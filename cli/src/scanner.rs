mod fs;
mod preview;
#[cfg(target_os = "macos")]
mod provider;
#[cfg(target_os = "macos")]
mod startup_volume;

use std::collections::HashSet;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use fs::{Directory, Identity, MetadataAccess, Operation, is_cloud_error, with_local_metadata};
use preview::Preview;

#[derive(Clone, Debug)]
pub struct Entry {
    pub path: PathBuf,
    pub allocated_bytes: u64,
    pub kind: EntryKind,
    pub state: EntryState,
    pub children: Vec<Arc<Entry>>,
}

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
    Incomplete,
    HardLinkAlias,
}

#[derive(Clone, Debug)]
pub struct Issue {
    pub path: PathBuf,
    pub message: String,
}

#[derive(Clone, Debug)]
pub struct Report {
    pub root: Arc<Entry>,
    pub issues: Vec<Issue>,
    pub is_preview: bool,
}

#[derive(Clone, Debug)]
pub enum Event {
    Progress { visited: u64, path: PathBuf },
    Snapshot(Report),
}

/// Scan read-only filesystem metadata. Progressive scans first resolve two levels
/// and then publish finished branches as they become available.
pub fn scan(
    path: &Path,
    progressive: bool,
    cancelled: &AtomicBool,
    notify: impl FnMut(Event),
) -> io::Result<Report> {
    fs::raise_open_file_limit();
    with_local_metadata(|| {
        check_cancelled(cancelled)?;
        let scope = scan_scope(path)?;
        let mut walker = Walker {
            notify,
            metadata: MetadataAccess::new(cancelled),
            cancelled,
            visited: 0,
            last_progress: None,
        };
        walker.progress(&scope, true);

        let mut preview = None;
        if progressive {
            let mut shallow = walker.walk(&scope, Some(2), &mut preview)?;
            if shallow.root.state != EntryState::Pending {
                return walker.complete(shallow);
            }
            shallow.is_preview = true;
            preview = Some(Preview::new(Arc::clone(&shallow.root)));
            (walker.notify)(Event::Snapshot(shallow));
        }

        let report = walker.walk(&scope, None, &mut preview)?;
        if progressive {
            walker.complete(report)
        } else {
            walker.progress(&scope, true);
            check_cancelled(cancelled)?;
            Ok(report)
        }
    })
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

struct Walker<'a, F> {
    notify: F,
    metadata: MetadataAccess<'a>,
    cancelled: &'a AtomicBool,
    visited: u64,
    last_progress: Option<Instant>,
}

impl<F: FnMut(Event)> Walker<'_, F> {
    fn walk(
        &mut self,
        scope: &Path,
        depth_limit: Option<usize>,
        preview: &mut Option<Preview>,
    ) -> io::Result<Report> {
        check_cancelled(self.cancelled)?;
        self.visited += 1;
        let metadata = match self.metadata.stat(scope, None, scope.as_os_str()) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::TimedOut => {
                return Ok(unavailable_root(scope, &error));
            }
            Err(error) => return Err(contextual_error(scope, Operation::Stat, &error)),
        };
        if metadata.st_mode & libc::S_IFMT != libc::S_IFDIR {
            let message = if metadata.st_mode & libc::S_IFMT == libc::S_IFLNK {
                "Choose the directory the symbolic link points to."
            } else {
                "Choose a directory."
            };
            return Err(io::Error::new(io::ErrorKind::InvalidInput, message));
        }
        if fs::is_dataless(&metadata) {
            return Ok(cloud_root(scope));
        }
        let device = metadata.st_dev;
        let identity = Identity::from(&metadata);
        let directory = match self.metadata.list(scope, None, scope.as_os_str(), identity) {
            Ok(directory) => directory,
            Err(error) if is_cloud_error(&error) => return Ok(cloud_root(scope)),
            Err(error) if error.kind() == io::ErrorKind::TimedOut => {
                return Ok(unavailable_root(scope, &error));
            }
            Err(error) => return Err(contextual_error(scope, Operation::List, &error)),
        };
        if let Some(error) = directory.error {
            return if is_cloud_error(&error) {
                Ok(cloud_root(scope))
            } else {
                Err(contextual_error(scope, Operation::List, &error))
            };
        }

        let mut issues = Vec::new();
        let mut links = HashSet::new();
        let mut ancestors = HashSet::from([identity]);
        #[cfg(target_os = "macos")]
        let startup_volume = startup_volume::StartupVolume::for_scope(scope, &directory.fd);
        let mut stack = vec![Frame::new(scope.to_path_buf(), identity, directory)];
        while !stack.is_empty() {
            check_cancelled(self.cancelled)?;
            let level = stack.len() - 1;
            let frame = &mut stack[level];
            let Some(name) = frame.names.next() else {
                let finished = stack.pop().expect("current frame exists");
                ancestors.remove(&finished.identity);
                let entry = finished.finish(&mut issues);
                if let Some(parent) = stack.last_mut() {
                    add(parent, entry, level - 1, preview);
                    self.publish_preview(preview, &issues);
                    continue;
                }
                return Ok(Report {
                    root: entry,
                    issues,
                    is_preview: false,
                });
            };

            let path = frame.path.join(&name);
            self.visited += 1;
            self.progress(&path, false);
            self.publish_preview(preview, &issues);
            let metadata = match self.metadata.stat(&path, Some(&frame.fd), &name) {
                Ok(metadata) => metadata,
                Err(error) => {
                    check_cancelled(self.cancelled)?;
                    record_issue(&mut issues, &path, Operation::Stat, &error);
                    frame.state = EntryState::Incomplete;
                    continue;
                }
            };
            if metadata.st_dev != device {
                continue;
            }

            match metadata.st_mode & libc::S_IFMT {
                libc::S_IFREG => {
                    let entry = file_entry(
                        path,
                        &metadata,
                        depth_limit.is_some(),
                        &mut links,
                        &mut issues,
                    );
                    add(frame, entry, level, preview);
                }
                libc::S_IFDIR => {
                    #[cfg(target_os = "macos")]
                    if let Some(volume) = &startup_volume
                        && volume.is_backing_path(&path)
                        && !fs::is_dataless(&metadata)
                    {
                        match self
                            .metadata
                            .canonical_directory_path(&path, &frame.fd, &name)
                        {
                            Ok(canonical) if volume.is_alias(&canonical) => continue,
                            Ok(_) => {}
                            Err(error) => {
                                check_cancelled(self.cancelled)?;
                                record_issue(&mut issues, &path, Operation::CanonicalPath, &error);
                                frame.state = EntryState::Incomplete;
                                continue;
                            }
                        }
                    }
                    let identity = Identity::from(&metadata);
                    if fs::is_dataless(&metadata) {
                        add(
                            frame,
                            empty_directory(path, EntryState::CloudOnly),
                            level,
                            preview,
                        );
                    } else if ancestors.contains(&identity) {
                        issues.push(Issue {
                            path: path.clone(),
                            message: "Skipped a directory that points back to an ancestor.".into(),
                        });
                        add(
                            frame,
                            empty_directory(path, EntryState::Incomplete),
                            level,
                            preview,
                        );
                    } else if depth_limit.is_some_and(|limit| level + 1 >= limit) {
                        add(
                            frame,
                            empty_directory(path, EntryState::Pending),
                            level,
                            preview,
                        );
                    } else {
                        match self.metadata.list(&path, Some(&frame.fd), &name, identity) {
                            Ok(directory)
                                if !directory.error.as_ref().is_some_and(is_cloud_error) =>
                            {
                                if let Some(error) = &directory.error {
                                    record_issue(&mut issues, &path, Operation::List, error);
                                }
                                ancestors.insert(identity);
                                stack.push(Frame::new(path, identity, directory));
                            }
                            Ok(_) => {
                                add(
                                    frame,
                                    empty_directory(path, EntryState::CloudOnly),
                                    level,
                                    preview,
                                );
                            }
                            Err(error) if is_cloud_error(&error) => {
                                add(
                                    frame,
                                    empty_directory(path, EntryState::CloudOnly),
                                    level,
                                    preview,
                                );
                            }
                            Err(error) => {
                                check_cancelled(self.cancelled)?;
                                record_issue(&mut issues, &path, Operation::List, &error);
                                add(
                                    frame,
                                    empty_directory(path, EntryState::Incomplete),
                                    level,
                                    preview,
                                );
                            }
                        }
                    }
                }
                // Symlinks and special filesystem objects contribute no bytes.
                _ => {}
            }
        }
        unreachable!("traversal finishes at the root");
    }

    fn progress(&mut self, path: &Path, force: bool) {
        let now = Instant::now();
        if !force
            && self
                .last_progress
                .is_some_and(|last| now - last < Duration::from_millis(100))
        {
            return;
        }
        self.last_progress = Some(now);
        (self.notify)(Event::Progress {
            visited: self.visited,
            path: path.to_path_buf(),
        });
    }

    fn publish_preview(&mut self, preview: &mut Option<Preview>, issues: &[Issue]) {
        if let Some(report) = preview.as_mut().and_then(|preview| preview.publish(issues)) {
            (self.notify)(Event::Snapshot(report));
        }
    }

    fn complete(&mut self, report: Report) -> io::Result<Report> {
        self.progress(&report.root.path, true);
        check_cancelled(self.cancelled)?;
        (self.notify)(Event::Snapshot(report.clone()));
        Ok(report)
    }
}

struct Frame {
    path: PathBuf,
    identity: Identity,
    fd: Arc<std::os::fd::OwnedFd>,
    names: std::vec::IntoIter<std::ffi::OsString>,
    children: Vec<Arc<Entry>>,
    state: EntryState,
}

impl Frame {
    fn new(path: PathBuf, identity: Identity, listing: Directory) -> Self {
        let state = if listing.error.is_some() {
            EntryState::Incomplete
        } else {
            EntryState::Local
        };
        Self {
            path,
            identity,
            fd: listing.fd,
            names: listing.names.into_iter(),
            children: Vec::new(),
            state,
        }
    }

    fn finish(self, issues: &mut Vec<Issue>) -> Arc<Entry> {
        let (entry, overflow) = directory_entry(self.path, self.state, self.children);
        if overflow {
            issues.push(Issue {
                path: entry.path.clone(),
                message: "Directory totals exceed the supported byte range.".into(),
            });
        }
        entry
    }
}

fn add(frame: &mut Frame, entry: Arc<Entry>, level: usize, preview: &mut Option<Preview>) {
    if level < 2
        && let Some(preview) = preview
    {
        preview.record(&frame.path, &entry);
    }
    frame.children.push(entry);
}

fn file_entry(
    path: PathBuf,
    metadata: &libc::stat,
    discovery: bool,
    links: &mut HashSet<Identity>,
    issues: &mut Vec<Issue>,
) -> Arc<Entry> {
    let mut state = if fs::is_dataless(metadata) {
        EntryState::CloudOnly
    } else if metadata.st_nlink > 1 && discovery {
        // An unseen deeper path may own this inode. Defer counting until the
        // ordered depth-first pass chooses the first link.
        EntryState::Pending
    } else if metadata.st_nlink > 1 && !links.insert(Identity::from(metadata)) {
        EntryState::HardLinkAlias
    } else {
        EntryState::Local
    };
    let mut allocated = 0;
    if state == EntryState::Local {
        // POSIX st_blocks describes 512-byte units, regardless of the filesystem block size.
        let blocks = metadata.st_blocks.max(0) as u64;
        allocated = match blocks.checked_mul(512) {
            Some(bytes) => bytes,
            None => {
                issues.push(Issue {
                    path: path.clone(),
                    message: "Allocated size exceeds the supported byte range.".into(),
                });
                state = EntryState::Incomplete;
                u64::MAX
            }
        };
    }
    Arc::new(Entry {
        path,
        allocated_bytes: allocated,
        kind: EntryKind::File,
        state,
        children: Vec::new(),
    })
}

fn empty_directory(path: PathBuf, state: EntryState) -> Arc<Entry> {
    directory_entry(path, state, Vec::new()).0
}

fn directory_entry(
    path: PathBuf,
    mut state: EntryState,
    children: Vec<Arc<Entry>>,
) -> (Arc<Entry>, bool) {
    let mut bytes = 0_u64;
    let mut overflow = false;
    for entry in &children {
        match bytes.checked_add(entry.allocated_bytes) {
            Some(total) => bytes = total,
            None => {
                bytes = u64::MAX;
                overflow = true;
            }
        }
        if entry.state == EntryState::Pending {
            state = EntryState::Pending;
        } else if entry.state == EntryState::Incomplete && state == EntryState::Local {
            state = EntryState::Incomplete;
        }
    }
    if overflow && state == EntryState::Local {
        state = EntryState::Incomplete;
    }
    (
        Arc::new(Entry {
            path,
            allocated_bytes: bytes,
            kind: EntryKind::Directory,
            state,
            children,
        }),
        overflow,
    )
}

fn unavailable_root(path: &Path, error: &io::Error) -> Report {
    Report {
        root: empty_directory(path.to_path_buf(), EntryState::Incomplete),
        issues: vec![Issue {
            path: path.to_path_buf(),
            message: error.to_string(),
        }],
        is_preview: false,
    }
}

fn cloud_root(path: &Path) -> Report {
    Report {
        root: empty_directory(path.to_path_buf(), EntryState::CloudOnly),
        issues: Vec::new(),
        is_preview: false,
    }
}

fn contextual_error(path: &Path, operation: Operation, error: &io::Error) -> io::Error {
    io::Error::new(
        error.kind(),
        format!("{operation} {path:?}: {}", metadata_error_message(error)),
    )
}

fn record_issue(issues: &mut Vec<Issue>, path: &Path, operation: Operation, error: &io::Error) {
    issues.push(Issue {
        path: path.to_path_buf(),
        message: format!("{operation}: {}", metadata_error_message(error)),
    });
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
