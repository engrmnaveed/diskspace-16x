mod details;
mod exclude;
mod fs;
mod parallel;
#[cfg(target_os = "macos")]
mod provider;
#[cfg(target_os = "macos")]
mod startup_volume;
mod tree;
mod walk;

use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub use details::Details;
pub use exclude::Excludes;
pub(crate) use fs::Identity;
use fs::{MetadataAccess, Operation, is_cloud_error, with_local_metadata};
pub use tree::{Entry, NodeId, Tree, TreeBuilder};
use walk::{Inline, Scope, Task};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntryKind {
    File,
    Directory,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntryState {
    Local,
    Pending,
    CloudOnly,
    /// The OS refused access to this entry or a descendant, and nothing failed otherwise.
    Protected,
    Incomplete,
    HardLinkAlias,
}

impl EntryState {
    /// Adds a skipped descendant's state to a directory's, keeping the more serious one.
    fn degrade(self, problem: Self) -> Self {
        match (self, problem) {
            (Self::Local, Self::Protected | Self::Incomplete) => problem,
            (Self::Protected, Self::Incomplete) => Self::Incomplete,
            _ => self,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Issue {
    pub path: PathBuf,
    pub message: String,
    pub kind: IssueKind,
}

impl Issue {
    fn error(path: PathBuf, message: impl Into<String>) -> Self {
        Self {
            path,
            message: message.into(),
            kind: IssueKind::Error,
        }
    }
}

/// Why a path was skipped. Access refusals are expected when scanning system locations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IssueKind {
    /// File permissions don't let this user read the path (EACCES).
    PermissionDenied,
    /// macOS privacy controls or System Integrity Protection refused access (EPERM).
    NotPermitted,
    /// Any other failure, such as an I/O error or an unresponsive cloud provider.
    Error,
}

impl IssueKind {
    pub(crate) fn of(error: &io::Error) -> Self {
        match error.raw_os_error() {
            Some(libc::EACCES) => Self::PermissionDenied,
            Some(libc::EPERM) => Self::NotPermitted,
            _ => Self::Error,
        }
    }

    /// The OS refused access, so the path is protected rather than failed.
    pub fn is_protected(self) -> bool {
        self != Self::Error
    }

    fn entry_state(self) -> EntryState {
        if self.is_protected() {
            EntryState::Protected
        } else {
            EntryState::Incomplete
        }
    }
}

#[derive(Clone, Debug)]
pub struct Report {
    pub tree: Arc<Tree>,
    pub issues: Vec<Issue>,
    /// Entries skipped by exclude patterns. Their contents aren't visited or counted.
    pub excluded_count: u64,
    pub is_preview: bool,
}

impl Report {
    pub fn root(&self) -> Entry<'_> {
        self.tree.root()
    }

    /// This report with the folder `subtree` scanned again, as from [`rescan`]. Totals,
    /// item counts, and states of every folder above it are recomputed, and issues at or
    /// below it are replaced. The excluded count keeps the earlier scan's figure. Returns
    /// None when the folder isn't in this report.
    pub fn replacing(&self, subtree: Report) -> Option<Report> {
        let folder = subtree.tree.root_path();
        if folder == self.tree.root_path() {
            return Some(Report {
                excluded_count: self.excluded_count.max(subtree.excluded_count),
                ..subtree
            });
        }
        let at = self.tree.find(folder)?;
        if self.tree.entry(at).kind() != EntryKind::Directory {
            return None;
        }
        let mut issues: Vec<Issue> = self
            .issues
            .iter()
            .filter(|issue| !issue.path.starts_with(folder))
            .cloned()
            .collect();
        issues.extend(subtree.issues);
        Some(Report {
            tree: Arc::new(self.tree.replacing(at, &subtree.tree)),
            issues,
            excluded_count: self.excluded_count,
            is_preview: self.is_preview,
        })
    }
}

#[derive(Clone, Debug)]
pub enum Event {
    Progress { visited: u64, path: PathBuf },
    Snapshot(Report),
}

/// The default number of threads that read metadata: one per core, up to 8. More
/// rarely helps, and one suits hard disks and network filesystems.
pub fn default_threads() -> usize {
    std::thread::available_parallelism().map_or(1, |cores| cores.get().min(8))
}

/// How a scan runs.
#[derive(Clone, Copy, Debug)]
pub struct Options<'a> {
    pub excludes: &'a Excludes,
    /// Threads that read metadata. One walks on the calling thread.
    pub threads: usize,
}

/// Scan read-only filesystem metadata. Progressive scans first resolve two levels
/// and then publish finished branches as they become available.
pub fn scan(
    path: &Path,
    options: Options<'_>,
    progressive: bool,
    cancelled: &AtomicBool,
    mut notify: impl FnMut(Event),
) -> io::Result<Report> {
    fs::raise_open_file_limit();
    with_local_metadata(|| {
        let scope = scan_scope(path)?;
        let report = walk(&scope, options, None, progressive, cancelled, &mut notify)?;
        if progressive {
            check_cancelled(cancelled)?;
            notify(Event::Snapshot(report.clone()));
        }
        Ok(report)
    })
}

/// Scan one folder of an earlier scan of `root` again, without previews. Exclude
/// patterns stay relative to `root`. Merge the result with [`Report::replacing`].
///
/// Hard links are counted once within the folder. A file that also has a link outside
/// it may be counted on both sides until a full rescan.
pub fn rescan(
    root: &Path,
    folder: &Path,
    options: Options<'_>,
    cancelled: &AtomicBool,
    mut notify: impl FnMut(Event),
) -> io::Result<Report> {
    fs::raise_open_file_limit();
    with_local_metadata(|| {
        check_cancelled(cancelled)?;
        let root = scan_scope(root)?;
        let scope = scan_scope(folder)?;
        if !scope.starts_with(&root) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "The folder isn't inside the scanned folder.",
            ));
        }
        let length = root.as_os_str().len();
        walk(&scope, options, Some(length), false, cancelled, &mut notify)
    })
}

/// Reads the root, then walks everything below it. Problems with the root itself fail
/// the scan, except a cloud-only root or an unresponsive cloud provider.
fn walk(
    scope: &Path,
    options: Options<'_>,
    exclude_root_length: Option<usize>,
    progressive: bool,
    cancelled: &AtomicBool,
    notify: &mut dyn FnMut(Event),
) -> io::Result<Report> {
    check_cancelled(cancelled)?;
    let progress = |notify: &mut dyn FnMut(Event), visited| {
        notify(Event::Progress {
            visited,
            path: scope.to_path_buf(),
        });
    };
    progress(notify, 1);
    let mut metadata = MetadataAccess::new(cancelled);
    let stat = match metadata.stat(scope, None, scope.as_os_str()) {
        Ok(stat) => stat,
        Err(error) if error.kind() == io::ErrorKind::TimedOut => {
            return Ok(unavailable_root(scope, &error));
        }
        Err(error) => return Err(contextual_error(scope, Operation::Stat, &error)),
    };
    if stat.st_mode & libc::S_IFMT != libc::S_IFDIR {
        let message = if stat.st_mode & libc::S_IFMT == libc::S_IFLNK {
            "Choose the directory the symbolic link points to."
        } else {
            "Choose a directory."
        };
        return Err(io::Error::new(io::ErrorKind::InvalidInput, message));
    }
    if fs::is_dataless(&stat) {
        return Ok(cloud_root(scope));
    }
    let identity = Identity::from(&stat);
    let directory = match metadata.list(scope, None, scope.as_os_str(), identity) {
        Ok(directory) => directory,
        Err(error) if is_cloud_error(&error) => return Ok(cloud_root(scope)),
        Err(error) if error.kind() == io::ErrorKind::TimedOut => {
            return Ok(unavailable_root(scope, &error));
        }
        Err(error) => return Err(contextual_error(scope, Operation::List, &error)),
    };
    if let Some(error) = &directory.error {
        return if is_cloud_error(error) {
            Ok(cloud_root(scope))
        } else {
            Err(contextual_error(scope, Operation::List, error))
        };
    }

    let walk_scope = Scope {
        root: scope.to_path_buf(),
        excludes: options.excludes,
        exclude_root_length: exclude_root_length.unwrap_or(scope.as_os_str().len()),
        device: stat.st_dev,
        root_details: Details::from_stat(&stat),
        cancelled,
        #[cfg(target_os = "macos")]
        startup_volume: startup_volume::StartupVolume::for_scope(scope, &directory.fd),
    };
    let (task, root) = Task::root(scope.to_path_buf(), identity, directory);
    let (report, visited) = if options.threads > 1 {
        let (report, visited) = parallel::run(
            &walk_scope,
            task,
            root,
            options.threads,
            progressive,
            notify,
        )?;
        (report, 1 + visited)
    } else {
        let mut inline = Inline {
            scope: &walk_scope,
            notify: &mut *notify,
            previews: progressive,
            visited: 1,
            last_progress: None,
        };
        let report = inline.run(task, root)?;
        (report, inline.visited)
    };
    progress(notify, visited);
    check_cancelled(cancelled)?;
    Ok(report)
}

/// The absolute path a scan of `path` reports as its root, without `.` components or
/// a trailing separator.
#[cfg(feature = "pro")]
pub fn scope(path: &Path) -> io::Result<PathBuf> {
    scan_scope(path)
}

fn scan_scope(path: &Path) -> io::Result<PathBuf> {
    if path.as_os_str().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Choose a directory.",
        ));
    }
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    // Preserve '..' after symlink parents. Components remove trailing separators so
    // the final-component no-follow check cannot be bypassed by a trailing slash.
    Ok(absolute
        .components()
        .filter(|part| *part != Component::CurDir)
        .collect())
}

/// The bytes of `path` after the scan root and its separator.
fn relative_bytes(scope_length: usize, path: &Path) -> &[u8] {
    let bytes = path.as_os_str().as_encoded_bytes();
    // A root of "/" has no separator of its own to skip.
    let start = if scope_length == 1 {
        1
    } else {
        scope_length + 1
    };
    bytes.get(start..).unwrap_or_default()
}

fn unavailable_root(path: &Path, error: &io::Error) -> Report {
    Report {
        tree: Arc::new(
            TreeBuilder::new(path.to_path_buf(), EntryState::Incomplete)
                .finish()
                .0,
        ),
        issues: vec![Issue::error(path.to_path_buf(), error.to_string())],
        excluded_count: 0,
        is_preview: false,
    }
}

fn cloud_root(path: &Path) -> Report {
    Report {
        tree: Arc::new(
            TreeBuilder::new(path.to_path_buf(), EntryState::CloudOnly)
                .finish()
                .0,
        ),
        issues: Vec::new(),
        excluded_count: 0,
        is_preview: false,
    }
}

fn contextual_error(path: &Path, operation: Operation, error: &io::Error) -> io::Error {
    io::Error::new(
        error.kind(),
        format!("{operation} {path:?}: {}", metadata_error_message(error)),
    )
}

fn record_issue(
    issues: &mut Vec<Issue>,
    path: &Path,
    operation: Operation,
    error: &io::Error,
) -> IssueKind {
    let kind = IssueKind::of(error);
    issues.push(Issue {
        path: path.to_path_buf(),
        message: format!("{operation}: {}", metadata_error_message(error)),
        kind,
    });
    kind
}

fn metadata_error_message(error: &io::Error) -> String {
    if is_cloud_error(error) {
        "Local metadata for this cloud path is unavailable.".into()
    } else {
        error.to_string()
    }
}

fn check_cancelled(cancelled: &AtomicBool) -> io::Result<()> {
    if cancelled.load(Ordering::Relaxed) {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "Scan cancelled.",
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
