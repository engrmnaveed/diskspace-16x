//! The walk on several threads. Workers take directory tasks from a shared queue while
//! the calling thread reports progress and publishes previews. The records they store
//! are composed in depth-first order afterwards, so the result doesn't depend on which
//! worker read which directory, or when.

use std::collections::VecDeque;
use std::io;
use std::mem;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use super::walk::{Scope, Slot, Task, Worker, finish, snapshot};
use super::{Event, Issue, Report, check_cancelled};

const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);
const PREVIEW_INTERVAL: Duration = Duration::from_millis(400);

#[derive(Default)]
struct Queue {
    /// The root and its folders, read first so a preview can show two levels.
    shallow: VecDeque<Task>,
    /// Everything deeper, taken last in first out to keep the walk close to depth first.
    deep: Vec<Task>,
    /// Deep tasks wait until the two-level preview is published.
    open: bool,
    active: usize,
    stop: bool,
    error: Option<io::Error>,
    /// A second-level folder finished since the last preview.
    finished: bool,
}

impl Queue {
    fn next(&mut self) -> Option<Task> {
        self.shallow
            .pop_front()
            .or_else(|| if self.open { self.deep.pop() } else { None })
    }

    fn fail(&mut self, error: io::Error) {
        self.error.get_or_insert(error);
        self.stop = true;
    }
}

struct Shared {
    queue: Mutex<Queue>,
    /// Workers wait here for tasks.
    work: Condvar,
    /// The calling thread waits here for progress, previews and the end.
    state: Condvar,
    visited: AtomicU64,
    excluded: AtomicU64,
    issues: Mutex<Vec<Issue>>,
    latest: Mutex<PathBuf>,
}

impl Shared {
    fn queue(&self) -> MutexGuard<'_, Queue> {
        self.queue.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn issues(&self) -> MutexGuard<'_, Vec<Issue>> {
        self.issues.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Stops the walk if a worker panics, so the others and the caller don't wait forever.
struct PanicGuard<'s>(&'s Shared);

impl Drop for PanicGuard<'_> {
    fn drop(&mut self) {
        if thread::panicking() {
            self.0
                .queue()
                .fail(io::Error::other("A scan thread stopped unexpectedly."));
            self.0.work.notify_all();
            self.0.state.notify_all();
        }
    }
}

/// Runs the walk on `threads` workers. Returns the report and the entries visited.
pub(super) fn run(
    scope: &Scope<'_>,
    task: Task,
    root: Arc<Slot>,
    threads: usize,
    previews: bool,
    notify: &mut dyn FnMut(Event),
) -> io::Result<(Report, u64)> {
    let shared = Shared {
        queue: Mutex::new(Queue {
            shallow: VecDeque::from([task]),
            open: !previews,
            ..Queue::default()
        }),
        work: Condvar::new(),
        state: Condvar::new(),
        visited: AtomicU64::new(0),
        excluded: AtomicU64::new(0),
        issues: Mutex::new(Vec::new()),
        latest: Mutex::new(scope.root.clone()),
    };
    let capacity = thread::scope(|threads_scope| {
        let mut workers = Vec::with_capacity(threads);
        for _ in 0..threads {
            let spawned = thread::Builder::new()
                .name("diskuse16x-walk".into())
                .spawn_scoped(threads_scope, || work(&shared, scope, previews));
            match spawned {
                Ok(worker) => workers.push(worker),
                Err(error) => {
                    // Fewer workers still finish the walk, but none can't.
                    if workers.is_empty() {
                        shared.queue().fail(error);
                    }
                    break;
                }
            }
        }
        coordinate(&shared, scope, &root, previews, notify);
        let mut capacity = (1, 0);
        for worker in workers {
            if let Ok((nodes, name_bytes)) = worker.join() {
                capacity.0 += nodes;
                capacity.1 += name_bytes;
            }
        }
        capacity
    });
    let queue = shared
        .queue
        .into_inner()
        .unwrap_or_else(PoisonError::into_inner);
    if let Some(error) = queue.error {
        return Err(error);
    }
    let issues = shared
        .issues
        .into_inner()
        .unwrap_or_else(PoisonError::into_inner);
    let report = finish(scope, root, issues, shared.excluded.into_inner(), capacity)?;
    Ok((report, shared.visited.into_inner()))
}

/// One worker: takes tasks until the walk stops. Returns the children it recorded
/// and the bytes of their names, which size the tree.
fn work(shared: &Shared, scope: &Scope<'_>, previews: bool) -> (usize, usize) {
    let _guard = PanicGuard(shared);
    let mut worker = Worker::new(scope.cancelled, previews);
    // Each thread turns off cloud downloads for its own metadata calls.
    let result = super::fs::with_local_metadata(|| {
        let mut queue = shared.queue();
        loop {
            let task = loop {
                if queue.stop {
                    return Ok(());
                }
                if let Some(task) = queue.next() {
                    break task;
                }
                queue = shared
                    .work
                    .wait(queue)
                    .unwrap_or_else(PoisonError::into_inner);
            };
            queue.active += 1;
            drop(queue);
            if let Ok(mut latest) = shared.latest.try_lock() {
                let latest = latest.as_mut_os_string();
                latest.clear();
                latest.push(task.path());
            }
            let result = worker.run(scope, task);
            shared
                .visited
                .fetch_add(mem::take(&mut worker.visited), Ordering::Relaxed);
            shared
                .excluded
                .fetch_add(mem::take(&mut worker.excluded), Ordering::Relaxed);
            if !worker.issues.is_empty() {
                shared.issues().append(&mut worker.issues);
            }
            queue = shared.queue();
            queue.active -= 1;
            let (tasks, finished) = match result {
                Ok(result) => result,
                Err(error) => return Err(error),
            };
            let added = tasks.len();
            // Tasks are pushed in reverse so the first name is taken first. Only the
            // root adds shallow tasks.
            for task in tasks.into_iter().rev() {
                if task.depth() < 2 {
                    queue.shallow.push_front(task);
                } else {
                    queue.deep.push(task);
                }
            }
            queue.finished |= finished;
            let idle = queue.active == 0 && queue.shallow.is_empty();
            if idle && queue.deep.is_empty() {
                queue.stop = true;
                shared.work.notify_all();
            } else {
                for _ in 0..added {
                    shared.work.notify_one();
                }
            }
            if idle || finished {
                shared.state.notify_all();
            }
        }
    });
    if let Err(error) = result {
        shared.queue().fail(error);
        shared.work.notify_all();
        shared.state.notify_all();
    }
    (worker.nodes, worker.name_bytes)
}

/// The calling thread's part: report progress, publish previews, and open the deep
/// tasks once the two-level preview is out.
fn coordinate(
    shared: &Shared,
    scope: &Scope<'_>,
    root: &Slot,
    previews: bool,
    notify: &mut dyn FnMut(Event),
) {
    let mut last_progress = Instant::now();
    let mut published: Option<Instant> = None;
    let mut queue = shared.queue();
    loop {
        if queue.stop {
            return;
        }
        if check_cancelled(scope.cancelled).is_err() {
            queue.fail(io::Error::new(
                io::ErrorKind::Interrupted,
                "Scan cancelled.",
            ));
            shared.work.notify_all();
            return;
        }
        let top_finished = !queue.open && queue.active == 0 && queue.shallow.is_empty();
        let preview_due = previews
            && (top_finished
                || (queue.finished
                    && queue.open
                    && published.is_some_and(|last| last.elapsed() >= PREVIEW_INTERVAL)));
        if preview_due {
            queue.finished = false;
            drop(queue);
            published = Some(Instant::now());
            let issues = shared.issues().clone();
            let excluded = shared.excluded.load(Ordering::Relaxed);
            let result = snapshot(scope, root, &issues, excluded);
            match result {
                Ok(report) => notify(Event::Snapshot(report)),
                Err(error) => shared.queue().fail(error),
            }
            queue = shared.queue();
            if top_finished {
                queue.open = true;
                shared.work.notify_all();
            }
            continue;
        }
        let (next, _) = shared
            .state
            .wait_timeout(queue, PROGRESS_INTERVAL)
            .unwrap_or_else(PoisonError::into_inner);
        queue = next;
        if last_progress.elapsed() >= PROGRESS_INTERVAL && !queue.stop {
            last_progress = Instant::now();
            drop(queue);
            let path = shared
                .latest
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone();
            notify(Event::Progress {
                visited: 1 + shared.visited.load(Ordering::Relaxed),
                path,
            });
            queue = shared.queue();
        }
    }
}
