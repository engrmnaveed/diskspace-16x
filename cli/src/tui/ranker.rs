use super::explorer::{LARGEST_LIMIT, Mode, RankKey, Sort};
use crate::{report::largest_files, scanner::Entry};
use std::{
    io,
    path::PathBuf,
    sync::{Arc, Condvar, Mutex, Weak},
    thread::{self, JoinHandle},
};

pub(super) struct RankRequest {
    pub(super) key: RankKey,
    pub(super) source: Arc<Entry>,
}

pub(super) struct Ranked {
    pub(super) key: RankKey,
    pub(super) folder: PathBuf,
    pub(super) snapshot: Weak<Entry>,
    pub(super) items: Vec<Arc<Entry>>,
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
    let items = match mode {
        Mode::Folders => {
            let mut items = request.source.children.clone();
            let by_path = |left: &Arc<Entry>, right: &Arc<Entry>| {
                left.path.as_os_str().cmp(right.path.as_os_str())
            };
            match sort {
                Sort::Size => items.sort_unstable_by(|left, right| {
                    metric
                        .bytes(right)
                        .cmp(&metric.bytes(left))
                        .then_with(|| by_path(left, right))
                }),
                Sort::Items => items.sort_unstable_by(|left, right| {
                    right
                        .item_count
                        .cmp(&left.item_count)
                        .then_with(|| by_path(left, right))
                }),
                Sort::Name => items.sort_by_cached_key(|entry| {
                    let name = entry.path.file_name().unwrap_or(entry.path.as_os_str());
                    (
                        name.to_string_lossy().to_lowercase(),
                        entry.path.as_os_str().to_owned(),
                    )
                }),
            }
            items
        }
        Mode::Largest => largest_files(&request.source, LARGEST_LIMIT, metric)
            .into_iter()
            .cloned()
            .collect(),
    };
    Ranked {
        key: request.key,
        folder: request.source.path.clone(),
        snapshot: Arc::downgrade(&request.source),
        items,
    }
}
