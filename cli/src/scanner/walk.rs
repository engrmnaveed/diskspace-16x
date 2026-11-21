//! The traversal. Each directory is one task: list it, read its children's metadata,
//! and store the result as a record. Subdirectories become new tasks. Records form a
//! tree that `compose` turns into a [`Tree`] in depth-first order, which is also where
//! hard links are assigned to their first path, so the result doesn't depend on the
//! order in which tasks ran.

use std::collections::{HashSet, VecDeque};
use std::ffi::OsStr;
use std::io;
use std::os::fd::OwnedFd;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use super::fs::{self, Directory, Identity, MetadataAccess, Operation, is_cloud_error};
#[cfg(target_os = "macos")]
use super::startup_volume::StartupVolume;
use super::tree::{NodeId, Tree, TreeBuilder};
use super::{
    EntryKind, EntryState, Event, Excludes, Issue, IssueKind, Report, check_cancelled,
    record_issue, relative_bytes,
};

/// What every task of one scan shares.
pub(super) struct Scope<'a> {
    pub(super) root: PathBuf,
    pub(super) excludes: &'a Excludes,
    /// Length of the path exclude patterns are relative to. A folder rescan keeps the
    /// original scan root, so anchored patterns such as `/build` still apply there.
    pub(super) exclude_root_length: usize,
    pub(super) device: libc::dev_t,
    pub(super) cancelled: &'a AtomicBool,
    #[cfg(target_os = "macos")]
    pub(super) startup_volume: Option<StartupVolume>,
}

/// One directory waiting to be listed.
pub(super) struct Task {
    path: PathBuf,
    /// The open parent directory. Children are opened relative to it, so a renamed or
    /// replaced ancestor can't redirect the scan. The root arrives already listed.
    parent: Option<Arc<OwnedFd>>,
    listing: Option<Directory>,
    identity: Identity,
    depth: usize,
    slot: Arc<Slot>,
    /// Identities of the directories above this one, to skip loops such as bind mounts.
    ancestors: Option<Arc<Ancestor>>,
    /// The second-level folder this task belongs to, while previews are published.
    segment: Option<Arc<Slot>>,
}

impl Task {
    pub(super) fn root(path: PathBuf, identity: Identity, listing: Directory) -> (Self, Arc<Slot>) {
        let slot = Arc::new(Slot::default());
        let task = Self {
            path,
            parent: None,
            listing: Some(listing),
            identity,
            depth: 0,
            slot: Arc::clone(&slot),
            ancestors: None,
            segment: None,
        };
        (task, slot)
    }

    pub(super) fn depth(&self) -> usize {
        self.depth
    }
}

struct Ancestor {
    identity: Identity,
    parent: Option<Arc<Ancestor>>,
}

/// Where a directory's record lands once its task runs.
#[derive(Default)]
pub(super) struct Slot {
    record: OnceLock<Record>,
    /// For a second-level folder while previews are on: tasks in its subtree that
    /// haven't finished. Zero means the whole subtree is recorded.
    outstanding: AtomicU32,
}

#[derive(Clone)]
struct Record {
    own_state: EntryState,
    names: Vec<u8>,
    children: Vec<Child>,
    /// Children with more than one hard link, by index, with their identities.
    links: Vec<(u32, Identity)>,
}

impl Drop for Record {
    /// Releases the records below this one with a loop rather than recursion, so a
    /// very deep tree can't overflow the stack.
    fn drop(&mut self) {
        let mut pending: Vec<Arc<Slot>> = self.take_folders().collect();
        while let Some(slot) = pending.pop() {
            if let Ok(mut slot) = Arc::try_unwrap(slot)
                && let Some(mut record) = slot.record.take()
            {
                pending.extend(record.take_folders());
            }
        }
    }
}

impl Record {
    fn take_folders(&mut self) -> impl Iterator<Item = Arc<Slot>> + '_ {
        self.children
            .iter_mut()
            .filter_map(|child| child.folder.take())
    }
}

#[derive(Clone)]
struct Child {
    name_end: u32,
    kind: EntryKind,
    /// A file's state, or the own state of a folder that has no task.
    state: EntryState,
    allocated: u64,
    apparent: u64,
    folder: Option<Arc<Slot>>,
}

/// What one thread collects while running tasks.
pub(super) struct Worker<'a> {
    metadata: MetadataAccess<'a>,
    pub(super) issues: Vec<Issue>,
    pub(super) excluded: u64,
    /// Entries visited since the last time the caller collected them.
    pub(super) visited: u64,
    /// Children recorded so far and the bytes of their names, to size the tree.
    pub(super) nodes: usize,
    pub(super) name_bytes: usize,
    previews: bool,
    path: PathBuf,
}

impl<'a> Worker<'a> {
    pub(super) fn new(cancelled: &'a AtomicBool, previews: bool) -> Self {
        Self {
            metadata: MetadataAccess::new(cancelled),
            issues: Vec::new(),
            excluded: 0,
            visited: 0,
            nodes: 0,
            name_bytes: 0,
            previews,
            path: PathBuf::new(),
        }
    }

    pub(super) fn current_path(&self) -> &Path {
        &self.path
    }

    /// Lists one directory and records its children. Returns its subdirectories as new
    /// tasks, in name order, and whether this finished a previewed second-level folder.
    pub(super) fn run(
        &mut self,
        scope: &Scope<'_>,
        mut task: Task,
    ) -> io::Result<(Vec<Task>, bool)> {
        check_cancelled(scope.cancelled)?;
        self.path.clone_from(&task.path);
        let listing = task.listing.take();
        let mut record = Record {
            own_state: EntryState::Local,
            names: Vec::new(),
            children: Vec::new(),
            links: Vec::new(),
        };
        let mut tasks = Vec::new();
        if let Some(directory) = self.open(scope, &task, listing, &mut record)? {
            self.read_children(scope, &task, directory, &mut record, &mut tasks)?;
        }
        self.nodes += record.children.len();
        self.name_bytes += record.names.len();
        task.slot
            .record
            .set(record)
            .unwrap_or_else(|_| unreachable!("each directory has one task"));
        let finished = task
            .segment
            .as_ref()
            .is_some_and(|segment| segment.outstanding.fetch_sub(1, Ordering::AcqRel) == 1);
        Ok((tasks, finished))
    }

    /// Opens and lists the task's directory, or records why it can't be read. The root
    /// arrives with its listing, which was read and checked before the walk started.
    fn open(
        &mut self,
        scope: &Scope<'_>,
        task: &Task,
        listing: Option<Directory>,
        record: &mut Record,
    ) -> io::Result<Option<Directory>> {
        if listing.is_some() {
            return Ok(listing);
        }
        let name = task.path.file_name().unwrap_or(task.path.as_os_str());
        let directory =
            match self
                .metadata
                .list(&task.path, task.parent.as_ref(), name, task.identity)
            {
                Ok(directory) if !directory.error.as_ref().is_some_and(is_cloud_error) => directory,
                Ok(_) => {
                    record.own_state = EntryState::CloudOnly;
                    return Ok(None);
                }
                Err(error) if is_cloud_error(&error) => {
                    record.own_state = EntryState::CloudOnly;
                    return Ok(None);
                }
                Err(error) => {
                    check_cancelled(scope.cancelled)?;
                    let kind = record_issue(&mut self.issues, &task.path, Operation::List, &error);
                    record.own_state = kind.entry_state();
                    return Ok(None);
                }
            };
        if let Some(error) = &directory.error {
            let kind = record_issue(&mut self.issues, &task.path, Operation::List, error);
            record.own_state = kind.entry_state();
        }
        Ok(Some(directory))
    }

    fn read_children(
        &mut self,
        scope: &Scope<'_>,
        task: &Task,
        directory: Directory,
        record: &mut Record,
        tasks: &mut Vec<Task>,
    ) -> io::Result<()> {
        let mut ancestors = None;
        let fd = directory.fd;
        // Most names become children, so size the record once rather than grow it.
        record.children.reserve_exact(directory.names.len());
        record
            .names
            .reserve_exact(directory.names.iter().map(|name| name.len()).sum());
        for name in &directory.names {
            check_cancelled(scope.cancelled)?;
            self.path.push(name);
            let result = self.read_child(scope, task, &fd, name, record, tasks, &mut ancestors);
            self.path.pop();
            result?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn read_child(
        &mut self,
        scope: &Scope<'_>,
        task: &Task,
        fd: &Arc<OwnedFd>,
        name: &OsStr,
        record: &mut Record,
        tasks: &mut Vec<Task>,
        ancestors: &mut Option<Arc<Ancestor>>,
    ) -> io::Result<()> {
        let relative = (!scope.excludes.is_empty())
            .then(|| relative_bytes(scope.exclude_root_length, &self.path));
        if relative.is_some_and(|relative| {
            scope
                .excludes
                .matches_any_type(relative, name.as_encoded_bytes())
        }) {
            self.excluded += 1;
            return Ok(());
        }
        self.visited += 1;
        let metadata = match self.metadata.stat(&self.path, Some(fd), name) {
            Ok(metadata) => metadata,
            Err(error) => {
                check_cancelled(scope.cancelled)?;
                let kind = record_issue(&mut self.issues, &self.path, Operation::Stat, &error);
                record.own_state = record.own_state.degrade(kind.entry_state());
                return Ok(());
            }
        };
        if metadata.st_dev != scope.device {
            return Ok(());
        }
        match metadata.st_mode & libc::S_IFMT {
            libc::S_IFREG => self.add_file(record, name, &metadata),
            libc::S_IFDIR => {
                if relative.is_some_and(|relative| {
                    scope
                        .excludes
                        .matches_directory(relative, name.as_encoded_bytes())
                }) {
                    self.excluded += 1;
                    return Ok(());
                }
                #[cfg(target_os = "macos")]
                if let Some(volume) = &scope.startup_volume
                    && volume.is_backing_path(&self.path)
                    && !fs::is_dataless(&metadata)
                {
                    match self.metadata.canonical_directory_path(&self.path, fd, name) {
                        Ok(canonical) if volume.is_alias(&canonical) => return Ok(()),
                        Ok(_) => {}
                        Err(error) => {
                            check_cancelled(scope.cancelled)?;
                            let kind = record_issue(
                                &mut self.issues,
                                &self.path,
                                Operation::CanonicalPath,
                                &error,
                            );
                            record.own_state = record.own_state.degrade(kind.entry_state());
                            return Ok(());
                        }
                    }
                }
                let identity = Identity::from(&metadata);
                if fs::is_dataless(&metadata) {
                    add_folder(record, name, EntryState::CloudOnly, None);
                    return Ok(());
                }
                let chain = ancestors.get_or_insert_with(|| {
                    Arc::new(Ancestor {
                        identity: task.identity,
                        parent: task.ancestors.clone(),
                    })
                });
                if loops_back(chain, identity) {
                    self.issues.push(Issue::error(
                        self.path.clone(),
                        "Skipped a directory that points back to an ancestor.",
                    ));
                    add_folder(record, name, EntryState::Incomplete, None);
                    return Ok(());
                }
                let slot = Arc::new(Slot::default());
                let depth = task.depth + 1;
                let segment = if !self.previews || depth < 2 {
                    None
                } else if depth == 2 {
                    slot.outstanding.store(1, Ordering::Relaxed);
                    Some(Arc::clone(&slot))
                } else {
                    let segment = task.segment.clone().expect("deep tasks have a segment");
                    segment.outstanding.fetch_add(1, Ordering::AcqRel);
                    Some(segment)
                };
                add_folder(record, name, EntryState::Local, Some(Arc::clone(&slot)));
                tasks.push(Task {
                    path: self.path.clone(),
                    parent: Some(Arc::clone(fd)),
                    listing: None,
                    identity,
                    depth,
                    slot,
                    ancestors: Some(Arc::clone(chain)),
                    segment,
                });
            }
            // Symlinks and special filesystem objects contribute no bytes.
            _ => {}
        }
        Ok(())
    }

    fn add_file(&mut self, record: &mut Record, name: &OsStr, metadata: &libc::stat) {
        let mut state = EntryState::Local;
        let mut allocated = 0;
        let mut apparent = 0;
        if fs::is_dataless(metadata) {
            state = EntryState::CloudOnly;
        } else {
            apparent = metadata.st_size.max(0) as u64;
            // POSIX st_blocks describes 512-byte units, regardless of the filesystem block size.
            let blocks = metadata.st_blocks.max(0) as u64;
            allocated = blocks.checked_mul(512).unwrap_or_else(|| {
                self.issues.push(Issue::error(
                    self.path.clone(),
                    "Allocated size exceeds the supported byte range.",
                ));
                state = EntryState::Incomplete;
                u64::MAX
            });
            if metadata.st_nlink > 1 {
                // Another path may own this inode. `compose` picks the first one.
                record
                    .links
                    .push((record.children.len() as u32, Identity::from(metadata)));
            }
        }
        record.names.extend_from_slice(name.as_bytes());
        record.children.push(Child {
            name_end: record.names.len() as u32,
            kind: EntryKind::File,
            state,
            allocated,
            apparent,
            folder: None,
        });
    }
}

fn add_folder(record: &mut Record, name: &OsStr, state: EntryState, folder: Option<Arc<Slot>>) {
    record.names.extend_from_slice(name.as_bytes());
    record.children.push(Child {
        name_end: record.names.len() as u32,
        kind: EntryKind::Directory,
        state,
        allocated: 0,
        apparent: 0,
        folder,
    });
}

fn loops_back(chain: &Ancestor, identity: Identity) -> bool {
    let mut current = Some(chain);
    while let Some(ancestor) = current {
        if ancestor.identity == identity {
            return true;
        }
        current = ancestor.parent.as_deref();
    }
    false
}

/// A record being composed: borrowed for a preview, or taken for the final tree so
/// each record is freed as soon as its folder is complete.
enum Held<'r> {
    Borrowed(&'r Record),
    Owned(Record),
}

impl Held<'_> {
    fn get(&self) -> &Record {
        match self {
            Held::Borrowed(record) => record,
            Held::Owned(record) => record,
        }
    }
}

/// The record in a slot nobody else holds. Tasks hold slots only while they run.
fn take_record(slot: Arc<Slot>) -> Option<Record> {
    match Arc::try_unwrap(slot) {
        Ok(slot) => slot.record.into_inner(),
        Err(shared) => shared.record.get().cloned(),
    }
}

/// Builds a preview tree from the records below `root`. Second-level folders whose
/// subtrees aren't finished show as pending.
pub(super) fn preview(path: &Path, root: &Slot) -> io::Result<Tree> {
    let record = root
        .record
        .get()
        .expect("the root is listed before previews");
    Ok(compose(path, Held::Borrowed(record), true, (0, 0))?.0)
}

/// Builds the final tree from the records below `root`, consuming them. Returns the
/// tree and the folders whose totals exceeded the byte range.
pub(super) fn complete(
    path: &Path,
    root: Arc<Slot>,
    capacity: (usize, usize),
) -> io::Result<(Tree, Vec<NodeId>)> {
    let record = take_record(root).expect("the root is listed before composing");
    compose(path, Held::Owned(record), false, capacity)
}

/// Walks the records depth first, which also assigns each hard-linked file to its
/// first path in that order.
fn compose(
    path: &Path,
    root: Held<'_>,
    preview: bool,
    capacity: (usize, usize),
) -> io::Result<(Tree, Vec<NodeId>)> {
    let own_state = root.get().own_state;
    let mut builder =
        TreeBuilder::with_capacity(path.to_path_buf(), own_state, capacity.0, capacity.1);
    let mut links = HashSet::new();
    // A pending folder earlier in depth-first order might hold the first link to a file.
    let mut pending_before = false;
    let mut stack = vec![(root, 0_usize)];
    while !stack.is_empty() {
        // Second-level folders are the ones a preview may show as pending.
        let second_level = stack.len() == 2;
        let top = stack.last_mut().expect("the stack isn't empty");
        let position = top.1;
        top.1 += 1;
        let record = top.0.get();
        let Some(child) = record.children.get(position) else {
            stack.pop();
            if !stack.is_empty() {
                builder.close_folder();
            }
            continue;
        };
        let start = match position {
            0 => 0,
            previous => record.children[previous - 1].name_end as usize,
        };
        let name = &record.names[start..child.name_end as usize];
        let slot = match (&child.folder, child.kind) {
            (_, EntryKind::File) => {
                let (mut state, mut allocated, mut apparent) =
                    (child.state, child.allocated, child.apparent);
                if let Ok(link) = record
                    .links
                    .binary_search_by_key(&(position as u32), |&(index, _)| index)
                {
                    if !links.insert(record.links[link].1) {
                        (state, allocated, apparent) = (EntryState::HardLinkAlias, 0, 0);
                    } else if pending_before {
                        (state, allocated, apparent) = (EntryState::Pending, 0, 0);
                    }
                }
                builder.file(name, allocated, apparent, state);
                continue;
            }
            (None, _) => {
                builder.folder(name, child.state);
                continue;
            }
            (Some(slot), _) => slot,
        };
        let unfinished = preview && second_level && slot.outstanding.load(Ordering::Acquire) != 0;
        let Some(own_state) = slot
            .record
            .get()
            .filter(|_| !unfinished)
            .map(|r| r.own_state)
        else {
            pending_before = true;
            builder.folder(name, EntryState::Pending);
            continue;
        };
        builder.open_folder(name, own_state);
        let next = match top.0 {
            Held::Borrowed(record) => record.children[position]
                .folder
                .as_ref()
                .and_then(|slot| slot.record.get())
                .map(Held::Borrowed),
            Held::Owned(ref mut record) => record.children[position]
                .folder
                .take()
                .and_then(take_record)
                .map(Held::Owned),
        };
        stack.push((next.expect("the folder's record was checked above"), 0));
    }
    builder.try_finish()
}

/// Runs the walk on the calling thread: first the root and its folders, so a preview
/// can show two levels, then everything below depth first.
pub(super) struct Inline<'s, 'a, F> {
    pub(super) scope: &'s Scope<'a>,
    pub(super) notify: F,
    pub(super) previews: bool,
    pub(super) visited: u64,
    pub(super) last_progress: Option<Instant>,
}

impl<F: FnMut(Event)> Inline<'_, '_, F> {
    pub(super) fn run(&mut self, task: Task, root: Arc<Slot>) -> io::Result<Report> {
        let scope = self.scope;
        let mut worker = Worker::new(scope.cancelled, self.previews);
        let mut shallow = VecDeque::from([task]);
        let mut deep = Vec::new();
        while let Some(task) = shallow.pop_front() {
            let (tasks, _) = worker.run(scope, task)?;
            self.collect(&mut worker);
            for task in tasks {
                if task.depth() < 2 {
                    shallow.push_back(task);
                } else {
                    deep.push(task);
                }
            }
        }
        let mut published = None;
        if self.previews && !deep.is_empty() {
            published = Some(Instant::now());
            let preview = self.snapshot(&worker, &root)?;
            (self.notify)(Event::Snapshot(preview));
        }
        deep.reverse();
        let mut changed = false;
        while let Some(task) = deep.pop() {
            let (tasks, finished) = worker.run(scope, task)?;
            self.collect(&mut worker);
            deep.extend(tasks.into_iter().rev());
            changed |= finished;
            if changed
                && let Some(last) = published
                && last.elapsed() >= Duration::from_millis(400)
                && !deep.is_empty()
            {
                published = Some(Instant::now());
                changed = false;
                let preview = self.snapshot(&worker, &root)?;
                (self.notify)(Event::Snapshot(preview));
            }
        }
        let capacity = (worker.nodes + 1, worker.name_bytes);
        finish(scope, root, vec![worker], capacity)
    }

    fn collect(&mut self, worker: &mut Worker<'_>) {
        self.visited += std::mem::take(&mut worker.visited);
        let now = Instant::now();
        if self
            .last_progress
            .is_some_and(|last| now - last < Duration::from_millis(100))
        {
            return;
        }
        self.last_progress = Some(now);
        (self.notify)(Event::Progress {
            visited: self.visited,
            path: worker.current_path().to_path_buf(),
        });
    }

    fn snapshot(&self, worker: &Worker<'_>, root: &Slot) -> io::Result<Report> {
        let tree = preview(&self.scope.root, root)?;
        let mut issues = worker.issues.clone();
        sort_issues(&mut issues);
        Ok(Report {
            tree: Arc::new(tree),
            issues,
            excluded_count: worker.excluded,
            is_preview: true,
        })
    }
}

/// Composes the final tree and gathers every worker's issues in path order.
pub(super) fn finish(
    scope: &Scope<'_>,
    root: Arc<Slot>,
    workers: Vec<Worker<'_>>,
    capacity: (usize, usize),
) -> io::Result<Report> {
    check_cancelled(scope.cancelled)?;
    let (tree, overflowed) = complete(&scope.root, root, capacity)?;
    let mut issues = Vec::new();
    let mut excluded_count = 0;
    for worker in workers {
        issues.extend(worker.issues);
        excluded_count += worker.excluded;
    }
    for id in overflowed {
        issues.push(Issue {
            path: tree.path(id),
            message: "Directory totals exceed the supported byte range.".into(),
            kind: IssueKind::Error,
        });
    }
    sort_issues(&mut issues);
    Ok(Report {
        tree: Arc::new(tree),
        issues,
        excluded_count,
        is_preview: false,
    })
}

/// Orders issues by path, comparing components as the depth-first walk visits them.
fn sort_issues(issues: &mut [Issue]) {
    issues.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.message.cmp(&right.message))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A chain of nested folders as records, deepest last.
    fn chain(depth: usize) -> Arc<Slot> {
        let mut below: Option<Arc<Slot>> = None;
        for _ in 0..depth {
            let slot = Arc::new(Slot::default());
            let children = below.take().map(|folder| Child {
                name_end: 1,
                kind: EntryKind::Directory,
                state: EntryState::Local,
                allocated: 0,
                apparent: 0,
                folder: Some(folder),
            });
            let record = Record {
                own_state: EntryState::Local,
                names: b"d".to_vec(),
                children: children.into_iter().collect(),
                links: Vec::new(),
            };
            assert!(slot.record.set(record).is_ok());
            below = Some(slot);
        }
        below.expect("a chain has a root")
    }

    #[test]
    fn handles_very_deep_trees_without_recursion() {
        // Recursive drops or walks would overflow a test thread's stack at this depth.
        drop(chain(200_000));
        let (tree, _) = complete(Path::new("/deep"), chain(200_000), (0, 0)).unwrap();
        assert_eq!(tree.root().item_count(), 199_999);
        assert_eq!(tree.root().descendants().count(), 199_999);
    }
}
