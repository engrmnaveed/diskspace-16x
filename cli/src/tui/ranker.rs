use super::explorer::{LARGEST_LIMIT, Mode, RankKey, Sort};
use crate::{
    report::largest_files,
    scanner::{Entry, NodeId, Tree},
};
use std::{
    io,
    os::unix::ffi::OsStrExt,
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
};

pub(super) struct RankRequest {
    pub(super) key: RankKey,
    pub(super) tree: Arc<Tree>,
    /// The folder to rank, or the whole scan for the largest files.
    pub(super) scope: NodeId,
}

pub(super) struct Ranked {
    pub(super) key: RankKey,
    pub(super) tree: Arc<Tree>,
    pub(super) folder: NodeId,
    pub(super) items: Vec<NodeId>,
}

#[derive(Default)]
struct Work {
    pending: Option<RankRequest>,
    finished: Option<Ranked>,
    stopped: bool,
}

pub(super) struct RankWorker {
    work: Arc<(Mutex<Work>, Condvar)>,
    thread: JoinHandle<()>,
}

impl RankWorker {
    pub(super) fn start() -> io::Result<Self> {
        let work = Arc::new((Mutex::new(Work::default()), Condvar::new()));
        let background = Arc::clone(&work);
        let thread = thread::Builder::new()
            .name("space16x-rank".into())
            .spawn(move || {
                let (queue, changed) = &*background;
                loop {
                    let guard = queue.lock().unwrap_or_else(|error| error.into_inner());
                    let mut state = changed
                        .wait_while(guard, |state| state.pending.is_none() && !state.stopped)
                        .unwrap_or_else(|error| error.into_inner());
                    if state.stopped {
                        return;
                    }
                    let request = state.pending.take().expect("a rank request was queued");
                    drop(state);
                    let result = rank(request);
                    let mut state = queue.lock().unwrap_or_else(|error| error.into_inner());
                    if state.stopped {
                        return;
                    }
                    let previous = state.finished.replace(result);
                    drop(state);
                    drop(previous);
                }
            })?;
        Ok(Self { work, thread })
    }

    pub(super) fn submit(&self, request: RankRequest) -> io::Result<()> {
        if self.thread.is_finished() {
            return Err(io::Error::other("The explorer ranking worker stopped."));
        }
        let (queue, changed) = &*self.work;
        let mut state = queue.lock().unwrap_or_else(|error| error.into_inner());
        let previous = state.pending.replace(request);
        drop(state);
        changed.notify_one();
        drop(previous);
        Ok(())
    }

    pub(super) fn take(&self) -> Option<Ranked> {
        self.work
            .0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .finished
            .take()
    }
}

impl Drop for RankWorker {
    fn drop(&mut self) {
        let (queue, changed) = &*self.work;
        let mut state = queue.lock().unwrap_or_else(|error| error.into_inner());
        state.stopped = true;
        let pending = state.pending.take();
        drop(state);
        changed.notify_one();
        drop(pending);
    }
}

pub(super) fn rank(request: RankRequest) -> Ranked {
    let RankKey { mode, sort, metric } = request.key;
    let scope = request.tree.entry(request.scope);
    let items = match mode {
        Mode::Folders => {
            let mut items: Vec<Entry<'_>> = scope.children().collect();
            // Siblings share a parent, so their names order them as their paths would.
            let by_name = |left: &Entry<'_>, right: &Entry<'_>| {
                left.name().as_bytes().cmp(right.name().as_bytes())
            };
            match sort {
                Sort::Size => items.sort_unstable_by(|left, right| {
                    metric
                        .bytes(*right)
                        .cmp(&metric.bytes(*left))
                        .then_with(|| by_name(left, right))
                }),
                Sort::Items => items.sort_unstable_by(|left, right| {
                    right
                        .item_count()
                        .cmp(&left.item_count())
                        .then_with(|| by_name(left, right))
                }),
                Sort::Name => items.sort_by_cached_key(|entry| {
                    let name = entry.name();
                    (name.to_string_lossy().to_lowercase(), name.to_owned())
                }),
            }
            items.into_iter().map(Entry::id).collect()
        }
        Mode::Largest => largest_files(scope, LARGEST_LIMIT, metric),
    };
    Ranked {
        key: request.key,
        folder: request.scope,
        tree: request.tree,
        items,
    }
}
