use std::collections::HashSet;
use std::io;
use std::marker::PhantomData;
use std::os::raw::c_int;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use super::check_cancelled;

// Constants and declarations from the macOS SDK's <sys/resource.h>.
const MATERIALIZATION_POLICY: c_int = 3;
const THREAD_SCOPE: c_int = 1;
const MATERIALIZATION_OFF: c_int = 1;

unsafe extern "C" {
    fn getiopolicy_np(kind: c_int, scope: c_int) -> c_int;
    fn setiopolicy_np(kind: c_int, scope: c_int, policy: c_int) -> c_int;
}

pub(super) struct LocalMetadataOnly {
    previous: c_int,
    thread_scope: PhantomData<Rc<()>>,
}

impl LocalMetadataOnly {
    pub(super) fn enter() -> io::Result<Self> {
        // SAFETY: these policy values are supported on macOS and only affect this thread.
        let previous = unsafe { getiopolicy_np(MATERIALIZATION_POLICY, THREAD_SCOPE) };
        if previous >= 0
            && unsafe { setiopolicy_np(MATERIALIZATION_POLICY, THREAD_SCOPE, MATERIALIZATION_OFF) }
                == 0
        {
            return Ok(Self {
                previous,
                thread_scope: PhantomData,
            });
        }
        let error = io::Error::last_os_error();
        Err(io::Error::new(
            error.kind(),
            format!("Could not disable cloud downloads: {error}"),
        ))
    }
}

impl Drop for LocalMetadataOnly {
    fn drop(&mut self) {
        // SAFETY: the guard is used synchronously on the thread that read this policy.
        unsafe { setiopolicy_np(MATERIALIZATION_POLICY, THREAD_SCOPE, self.previous) };
    }
}

pub(super) fn is_provider_path(path: &Path) -> bool {
    let mut library = false;
    for component in path.components() {
        let component = component.as_os_str();
        if library && (component == "Mobile Documents" || component == "CloudStorage") {
            return true;
        }
        library = component == "Library";
    }
    false
}

pub(super) struct Access<'a> {
    cancelled: &'a AtomicBool,
    timed_out: HashSet<PathBuf>,
}

impl<'a> Access<'a> {
    pub(super) fn new(cancelled: &'a AtomicBool) -> Self {
        Self {
            cancelled,
            timed_out: HashSet::new(),
        }
    }

    pub(super) fn run<T: Send + 'static>(
        &mut self,
        path: &Path,
        operation: impl FnOnce(&AtomicBool) -> io::Result<T> + Send + 'static,
    ) -> io::Result<T> {
        check_cancelled(self.cancelled)?;
        if self.timed_out.contains(path) {
            return Err(metadata_timeout());
        }
        // Scan threads take turns, so a busy pool doesn't fail calls that never ran.
        let _turn = TURN.lock().unwrap_or_else(PoisonError::into_inner);
        check_cancelled(self.cancelled)?;

        let pool = POOL.get_or_init(Pool::new);
        let slot = pool.try_acquire().ok_or_else(metadata_timeout)?;
        let abandoned = Arc::new(AtomicBool::new(false));
        let _request = Request(Arc::clone(&abandoned));
        let (send, receive) = mpsc::channel();
        let deadline = Instant::now() + Duration::from_secs(1);
        pool.jobs
            .try_send(Box::new(move || {
                let result = (|| {
                    let _local_only = LocalMetadataOnly::enter()?;
                    check_cancelled(&abandoned)?;
                    operation(&abandoned)
                })();
                drop(slot);
                let _ = send.send(result);
            }))
            .map_err(|_| metadata_timeout())?;

        loop {
            check_cancelled(self.cancelled)?;
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                self.timed_out.insert(path.to_path_buf());
                return Err(metadata_timeout());
            }
            match receive.recv_timeout(remaining.min(Duration::from_millis(25))) {
                Ok(result) => return result,
                Err(mpsc::RecvTimeoutError::Disconnected) => return Err(metadata_timeout()),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
    }
}

fn metadata_timeout() -> io::Error {
    io::Error::new(
        io::ErrorKind::TimedOut,
        "The cloud provider did not return local metadata in time.",
    )
}

struct Request(Arc<AtomicBool>);

impl Drop for Request {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

type Job = Box<dyn FnOnce() + Send>;
const WORKERS: usize = 2;
static POOL: OnceLock<Pool> = OnceLock::new();
static TURN: Mutex<()> = Mutex::new(());

/// Provider calls may still be blocked in the kernel after our wait ends. Keep
/// one slot occupied until its actual worker returns, with no unbounded backlog.
struct Pool {
    jobs: mpsc::SyncSender<Job>,
    available: AtomicUsize,
}

impl Pool {
    fn new() -> Self {
        let (jobs, queue) = mpsc::sync_channel::<Job>(WORKERS);
        let queue = Arc::new(Mutex::new(queue));
        let mut started = 0;
        for _ in 0..WORKERS {
            let queue = Arc::clone(&queue);
            if thread::Builder::new()
                .name("diskuse16x-metadata".into())
                .spawn(move || {
                    loop {
                        let job = queue.lock().expect("provider job queue").recv();
                        match job {
                            Ok(job) => job(),
                            Err(_) => break,
                        }
                    }
                })
                .is_ok()
            {
                started += 1;
            }
        }
        Self {
            jobs,
            available: AtomicUsize::new(started),
        }
    }

    fn try_acquire(&'static self) -> Option<Slot> {
        self.available
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |available| {
                available.checked_sub(1)
            })
            .ok()
            .map(|_| Slot(&self.available))
    }
}

struct Slot(&'static AtomicUsize);

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_worker_disables_materialization() -> io::Result<()> {
        let path = Path::new("/Users/test/Library/CloudStorage/Provider");
        assert!(is_provider_path(path));
        assert!(!is_provider_path(Path::new(
            "/Users/test/Library/CloudStorage-old"
        )));
        let cancelled = AtomicBool::new(false);
        let policy = Access::new(&cancelled).run(path, |_| {
            // SAFETY: fixed valid policy constants query the worker's current thread.
            Ok(unsafe { getiopolicy_np(MATERIALIZATION_POLICY, THREAD_SCOPE) })
        })?;
        assert_eq!(policy, MATERIALIZATION_OFF);
        Ok(())
    }

    #[test]
    fn provider_deadline_returns_before_the_worker_and_avoids_retrying_that_path() -> io::Result<()>
    {
        let path = Path::new("/Users/test/Library/Mobile Documents/provider-test");
        let cancelled = AtomicBool::new(false);
        let mut metadata = Access::new(&cancelled);
        let (release, hold) = mpsc::channel();
        let result = metadata.run(path, move |abandoned| {
            let _ = hold.recv();
            check_cancelled(abandoned)
        });
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::TimedOut);
        let retry = metadata.run(path, |_| Ok(()));
        assert_eq!(retry.unwrap_err().kind(), io::ErrorKind::TimedOut);
        release.send(()).map_err(io::Error::other)?;
        Ok(())
    }
}
