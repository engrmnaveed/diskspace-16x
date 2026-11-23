use super::ranker::{RankRequest, Ranked};
use super::{Progress, ScanUpdate};
use crate::{
    report::{Metric, format_bytes, terminal_path},
    scanner::{Entry, EntryKind, EntryState, NodeId, Report, Tree},
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::{
    collections::BTreeSet,
    io,
    path::{Path, PathBuf},
    sync::{Arc, Weak},
};

pub(super) const LARGEST_LIMIT: usize = 100;
const FILTER_LIMIT: usize = 256;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    Folders,
    Largest,
    /// Pro: cleanup suggestions in the whole scan.
    #[cfg(feature = "pro")]
    Cleanup,
    /// Pro: files with the same contents, once the user starts comparing them.
    #[cfg(feature = "pro")]
    Duplicates,
}

/// How folder contents are ordered. The largest-files view always ranks by size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Sort {
    #[default]
    Size,
    Items,
    Name,
}

impl Sort {
    fn next(self) -> Self {
        match self {
            Self::Size => Self::Items,
            Self::Items => Self::Name,
            Self::Name => Self::Size,
        }
    }
}

/// What a ranking was computed for. A change in any part needs a new ranking.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct RankKey {
    pub(super) mode: Mode,
    pub(super) sort: Sort,
    pub(super) metric: Metric,
}

pub(super) enum Status {
    Scanning,
    Cancelling,
    Complete,
    Cancelled,
    Failed(io::Error),
}

pub(super) enum Panel {
    Help,
    Issues,
    Details,
    /// Pro: the open folder's file types, largest first.
    #[cfg(feature = "pro")]
    Types(Vec<crate::pro::TypeTotal>),
    /// Pro: the space on the open folder's volume that a scan can't see, line by line.
    #[cfg(feature = "pro")]
    Explain(Vec<String>),
}

pub(super) enum Action {
    Stay,
    CancelScan,
    /// Move these items to the Trash, then report the results to `finish_trash`.
    Trash(Vec<TrashItem>),
    /// Scan `folder` again, with exclude patterns relative to `root`, and report the
    /// result to `finish`.
    Rescan {
        root: PathBuf,
        folder: PathBuf,
    },
    Quit,
}

/// An item chosen for the Trash, as the scan saw it.
#[derive(Clone, Debug)]
pub(super) struct TrashItem {
    pub(super) path: PathBuf,
    pub(super) kind: EntryKind,
    pub(super) allocated_bytes: u64,
    state: EntryState,
}

impl TrashItem {
    fn new(entry: Entry<'_>) -> Self {
        Self {
            path: entry.path(),
            kind: entry.kind(),
            allocated_bytes: entry.allocated_bytes(),
            state: entry.state(),
        }
    }

    /// The quoted name for messages.
    pub(super) fn quoted_name(&self) -> String {
        quoted_name(&self.path)
    }
}

fn quoted_name(path: &Path) -> String {
    format!(
        "“{}”",
        terminal_path(path.file_name().map_or(path, Path::new))
    )
}

/// Items waiting for the user to confirm Move to Trash, with their allocated bytes.
pub(super) struct TrashPlan {
    pub(super) items: Vec<TrashItem>,
    pub(super) bytes: u64,
    /// Pro: a plain warning when the items are or hold folders that can't be recreated,
    /// such as Xcode archives.
    pub(super) warning: Option<String>,
}

/// A message shown in the footer until the next key press.
pub(super) struct Notice {
    pub(super) text: String,
    pub(super) warning: bool,
}

impl Notice {
    fn info(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            warning: false,
        }
    }

    fn warning(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            warning: true,
        }
    }
}

const READ_ONLY_NOTICE: &str = "Read-only mode: Move to Trash is off.";
const IMPORTED_NOTICE: &str =
    "This scan was imported, so it's read-only. Scan the folder to change it.";

pub(super) struct Explorer {
    pub(super) requested_path: PathBuf,
    pub(super) report: Option<Report>,
    pub(super) status: Status,
    pub(super) progress: Option<Progress>,
    pub(super) mode: Mode,
    pub(super) sort: Sort,
    pub(super) metric: Metric,
    /// The visible rows, as nodes of the latest ranking's tree.
    pub(super) rows: Vec<NodeId>,
    pub(super) selected: usize,
    pub(super) offset: usize,
    pub(super) page_rows: usize,
    pub(super) query: String,
    pub(super) original_query: Option<String>,
    pub(super) panel: Option<Panel>,
    pub(super) panel_offset: usize,
    pub(super) panel_items: usize,
    pub(super) panel_page: usize,
    /// Set by `--read-only` or `DISKUSE16X_READ_ONLY`: marking and trashing are off.
    pub(super) read_only: bool,
    /// Pro: the folder view draws its contents as a treemap instead of a list.
    #[cfg(feature = "pro")]
    pub(super) treemap: bool,
    /// Pro: the rows show an Exclusive column, measured after the scan.
    #[cfg(feature = "pro")]
    pub(super) exclusive_column: bool,
    /// Pro: exclusive sizes of the latest finished scan, while the column is on.
    #[cfg(feature = "pro")]
    exclusive: Option<crate::pro::ExclusiveMeasurement>,
    /// Pro: whether the finished measurement has been drawn.
    #[cfg(feature = "pro")]
    exclusive_drawn: bool,
    /// Pro: the duplicate search the user started in the Duplicates view.
    #[cfg(feature = "pro")]
    duplicates: Option<crate::pro::DuplicateSearch>,
    /// Pro: notes for the Duplicates view's rows, such as "3 copies, 2.0 MB extra".
    #[cfg(feature = "pro")]
    pub(super) duplicate_labels: std::collections::HashMap<NodeId, String>,
    /// Pro: the Duplicates view's group count and extra space.
    #[cfg(feature = "pro")]
    pub(super) duplicate_summary: (usize, u64),
    /// Pro: whether the finished search's unreadable files were reported.
    #[cfg(feature = "pro")]
    duplicates_reported: bool,
    /// Pro: the Explain panel's lines, once the system answers.
    #[cfg(feature = "pro")]
    explain_job: Option<Arc<std::sync::OnceLock<Vec<String>>>>,
    /// The scan's thread count, which measuring exclusive sizes uses too.
    #[cfg_attr(not(feature = "pro"), allow(dead_code))]
    pub(super) threads: usize,
    /// A saved scan from `--import`. Its files may be elsewhere, so it is read-only and
    /// can't be rescanned.
    pub(super) imported: bool,
    /// Paths marked for the Trash. They can span folders and both views.
    pub(super) marked: BTreeSet<PathBuf>,
    pub(super) prompt: Option<TrashPlan>,
    pub(super) notice: Option<Notice>,
    /// The open folder in `report`.
    folder: Option<NodeId>,
    /// The folder being scanned again, and the status to return to afterwards.
    rescan: Option<(PathBuf, Status)>,
    requested_ranking: Option<(RankKey, Weak<Tree>, NodeId)>,
    ranking: Option<Ranked>,
    selection_target: Option<PathBuf>,
}

impl Explorer {
    pub(super) fn new(path: &Path) -> Self {
        Self {
            requested_path: path.to_path_buf(),
            report: None,
            status: Status::Scanning,
            progress: None,
            mode: Mode::Folders,
            sort: Sort::default(),
            metric: Metric::default(),
            rows: Vec::new(),
            selected: 0,
            offset: 0,
            page_rows: 1,
            query: String::new(),
            original_query: None,
            panel: None,
            panel_offset: 0,
            panel_items: 0,
            panel_page: 1,
            read_only: false,
            #[cfg(feature = "pro")]
            treemap: false,
            #[cfg(feature = "pro")]
            exclusive_column: false,
            #[cfg(feature = "pro")]
            exclusive: None,
            #[cfg(feature = "pro")]
            exclusive_drawn: false,
            #[cfg(feature = "pro")]
            duplicates: None,
            #[cfg(feature = "pro")]
            duplicate_labels: std::collections::HashMap::new(),
            #[cfg(feature = "pro")]
            duplicate_summary: (0, 0),
            #[cfg(feature = "pro")]
            duplicates_reported: false,
            #[cfg(feature = "pro")]
            explain_job: None,
            threads: 1,
            imported: false,
            marked: BTreeSet::new(),
            prompt: None,
            notice: None,
            folder: None,
            rescan: None,
            requested_ranking: None,
            ranking: None,
            selection_target: None,
        }
    }

    pub(super) fn is_active(&self) -> bool {
        matches!(self.status, Status::Scanning | Status::Cancelling)
    }

    pub(super) fn apply(&mut self, update: ScanUpdate) -> bool {
        let mut changed = false;
        if let Some(progress) = update.progress {
            self.progress = Some(progress);
            changed = true;
        }
        if let Some(report) = update.preview {
            self.install(report);
            changed = true;
        }
        if let Some(result) = update.finished {
            self.finish(result);
            changed = true;
        }
        changed
    }

    pub(super) fn is_rescanning(&self) -> bool {
        self.rescan.is_some()
    }

    pub(super) fn finish(&mut self, result: io::Result<Report>) {
        if let Some((folder, previous)) = self.rescan.take() {
            self.finish_rescan(&folder, previous, result);
            return;
        }
        self.status = match result {
            Ok(report) => {
                self.install(report);
                // Files may have changed since an earlier search.
                #[cfg(feature = "pro")]
                {
                    self.duplicates = None;
                }
                Status::Complete
            }
            Err(error)
                if error.kind() == io::ErrorKind::Interrupted
                    && matches!(self.status, Status::Cancelling) =>
            {
                Status::Cancelled
            }
            Err(error) => Status::Failed(error),
        };
    }

    pub(super) fn quit(self) -> io::Result<()> {
        match self.status {
            Status::Failed(error) => Err(error),
            _ => Ok(()),
        }
    }

    /// The folder being browsed, or the whole scan in the largest files view.
    pub(super) fn scope(&self) -> Option<Entry<'_>> {
        let report = self.report.as_ref()?;
        Some(report.tree.entry(self.scope_id()?))
    }

    fn scope_id(&self) -> Option<NodeId> {
        self.report.as_ref()?;
        match self.mode {
            Mode::Folders => self.folder,
            _ => Some(NodeId::ROOT),
        }
    }

    fn rank_key(&self) -> RankKey {
        RankKey {
            mode: self.mode,
            sort: self.sort,
            metric: self.metric,
        }
    }

    pub(super) fn ranking_request(&mut self) -> Option<RankRequest> {
        // Pro's Duplicates view fills its rows from the search instead.
        if self.is_duplicates_view() {
            return None;
        }
        let tree = Arc::clone(&self.report.as_ref()?.tree);
        let scope = self.scope_id()?;
        let key = self.rank_key();
        if self
            .requested_ranking
            .as_ref()
            .is_some_and(|(requested_key, requested, folder)| {
                *requested_key == key
                    && *folder == scope
                    && Weak::as_ptr(requested) == Arc::as_ptr(&tree)
            })
        {
            return None;
        }
        self.requested_ranking = Some((key, Arc::downgrade(&tree), scope));
        Some(RankRequest { key, tree, scope })
    }

    pub(super) fn is_ranking(&self) -> bool {
        let (Some(report), Some(scope)) = (&self.report, self.scope_id()) else {
            return false;
        };
        if self.is_duplicates_view() {
            return false;
        }
        self.ranking.as_ref().is_none_or(|rank| {
            rank.key != self.rank_key()
                || rank.folder != scope
                || !Arc::ptr_eq(&rank.tree, &report.tree)
        })
    }

    pub(super) fn accept_ranking(&mut self, ranked: Ranked) -> bool {
        if ranked.key != self.rank_key()
            || self
                .scope()
                .is_none_or(|scope| scope.path() != ranked.tree.path(ranked.folder))
        {
            return false;
        }
        let preferred = self
            .selection_target
            .clone()
            .or_else(|| self.current().map(Entry::path));
        self.ranking = Some(ranked);
        self.rebuild(preferred.as_deref());
        if !self.is_ranking()
            || self
                .current()
                .is_some_and(|row| Some(row.path()) == self.selection_target)
        {
            self.selection_target = None;
        }
        true
    }

    /// Whether Pro's Duplicates view is open.
    pub(super) fn is_duplicates_view(&self) -> bool {
        #[cfg(feature = "pro")]
        return self.mode == Mode::Duplicates;
        #[cfg(not(feature = "pro"))]
        false
    }

    /// The tree the rows belong to.
    pub(super) fn rows_tree(&self) -> Option<&Tree> {
        self.ranking.as_ref().map(|ranking| &*ranking.tree)
    }

    pub(super) fn current(&self) -> Option<Entry<'_>> {
        let id = *self.rows.get(self.selected)?;
        Some(self.rows_tree()?.entry(id))
    }

    /// A row's name in the folder view, or its path below the scope in the views of the
    /// whole scan.
    pub(super) fn row_path(&self, entry: Entry<'_>) -> PathBuf {
        match self.mode {
            Mode::Folders => PathBuf::from(entry.name()),
            _ => entry.tree().relative_path(entry.id(), NodeId::ROOT),
        }
    }

    pub(super) fn set_page_rows(&mut self, height: usize) {
        self.page_rows = height.max(1);
        if self.selected < self.offset {
            self.offset = self.selected;
        } else if self.selected >= self.offset.saturating_add(self.page_rows) {
            self.offset = (self.selected + 1).saturating_sub(self.page_rows);
        }
        self.offset = self
            .offset
            .min(self.rows.len().saturating_sub(self.page_rows));
    }

    fn install(&mut self, report: Report) {
        let old_folder = self
            .report
            .as_ref()
            .zip(self.folder)
            .map(|(old, folder)| old.tree.path(folder));
        let tree = &report.tree;
        let mut folder_path = old_folder
            .clone()
            .unwrap_or_else(|| tree.root_path().to_path_buf());
        let folder = loop {
            if let Some(folder) = tree.find(&folder_path) {
                break folder;
            }
            if !folder_path.pop() || !folder_path.starts_with(tree.root_path()) {
                break NodeId::ROOT;
            }
        };
        if self.mode == Mode::Folders && old_folder.is_some_and(|old| old != tree.path(folder)) {
            self.reset_ranking(None);
        }
        self.folder = Some(folder);
        self.report = Some(report);
    }

    fn rebuild(&mut self, preferred: Option<&Path>) {
        let (rows, target) = match &self.ranking {
            None => (Vec::new(), None),
            Some(ranking) => {
                let rows = if self.query.is_empty() {
                    ranking.items.clone()
                } else {
                    let needle = self.query.to_lowercase();
                    ranking
                        .items
                        .iter()
                        .copied()
                        .filter(|&id| {
                            terminal_path(&self.row_path(ranking.tree.entry(id)))
                                .to_lowercase()
                                .contains(&needle)
                        })
                        .collect()
                };
                (rows, preferred.and_then(|path| ranking.tree.find(path)))
            }
        };
        self.selected = target
            .and_then(|target| rows.iter().position(|&row| row == target))
            .unwrap_or_else(|| self.selected.min(rows.len().saturating_sub(1)));
        self.rows = rows;
        self.set_page_rows(self.page_rows);
    }

    fn refresh(&mut self) {
        let previous = self.current().map(Entry::path);
        self.rebuild(previous.as_deref());
    }

    fn reset_ranking(&mut self, selected: Option<&Path>) {
        self.requested_ranking = None;
        self.ranking = None;
        self.selection_target = selected.map(Path::to_path_buf);
        self.rows.clear();
        self.selected = 0;
        self.offset = 0;
    }

    fn switch_folder(&mut self, path: &Path, selected: Option<&Path>) {
        let Some(folder) = self
            .report
            .as_ref()
            .and_then(|report| report.tree.find(path))
        else {
            return;
        };
        self.folder = Some(folder);
        self.mode = Mode::Folders;
        self.query.clear();
        self.reset_ranking(selected);
    }

    fn activate(&mut self) {
        let Some(entry) = self.current() else { return };
        let (kind, path) = (entry.kind(), entry.path());
        match (self.mode, kind) {
            (Mode::Folders, EntryKind::Directory) => self.switch_folder(&path, None),
            (Mode::Folders, EntryKind::File) => self.open_panel(Panel::Details),
            _ => {
                if let Some(parent) = path.parent() {
                    self.switch_folder(parent, Some(&path));
                }
            }
        }
    }

    fn back(&mut self) {
        if !self.query.is_empty() {
            self.query.clear();
            self.refresh();
        } else if self.mode != Mode::Folders {
            self.mode = Mode::Folders;
            self.reset_ranking(None);
        } else if let (Some(report), Some(folder)) = (&self.report, self.folder)
            && folder != NodeId::ROOT
        {
            let child = report.tree.path(folder);
            if let Some(parent) = child.parent() {
                self.switch_folder(parent, Some(&child));
            }
        }
    }

    fn toggle_largest(&mut self) {
        if self.report.is_none() {
            return;
        }
        self.mode = match self.mode {
            Mode::Largest => Mode::Folders,
            _ => Mode::Largest,
        };
        self.query.clear();
        self.reset_ranking(None);
    }

    /// Pro: switches the folder view between the list and a treemap of its contents.
    #[cfg(feature = "pro")]
    fn toggle_treemap(&mut self) {
        if self.report.is_none() {
            return;
        }
        if self.mode != Mode::Folders {
            self.mode = Mode::Folders;
            self.query.clear();
            self.reset_ranking(None);
            self.treemap = true;
        } else {
            self.treemap = !self.treemap;
        }
    }

    /// Pro: lists the folders in the scan that tools recreate, for Move to Trash.
    #[cfg(feature = "pro")]
    fn toggle_cleanup(&mut self) {
        if self.report.is_none() {
            return;
        }
        self.mode = match self.mode {
            Mode::Cleanup => Mode::Folders,
            _ => Mode::Cleanup,
        };
        self.query.clear();
        self.reset_ranking(None);
    }

    /// Pro: opens or leaves the Duplicates view. It reads nothing until the user starts
    /// a search there with Enter.
    #[cfg(feature = "pro")]
    fn toggle_duplicates(&mut self) {
        if self.report.is_none() {
            return;
        }
        self.mode = match self.mode {
            Mode::Duplicates => Mode::Folders,
            _ => Mode::Duplicates,
        };
        self.query.clear();
        self.reset_ranking(None);
    }

    /// Pro: compares the finished scan's files of 1 MB or more by contents, in the
    /// background. Only this reads file contents, and only because the user asked.
    #[cfg(feature = "pro")]
    fn start_duplicates(&mut self) {
        let Some(report) = &self.report else { return };
        if self.imported {
            self.notice = Some(Notice::info(
                "Duplicates need the files themselves. Scan the folder to find them.",
            ));
            return;
        }
        if report.is_preview || self.is_active() {
            self.notice = Some(Notice::info(
                "Duplicates can be found when the scan finishes.",
            ));
            return;
        }
        match crate::pro::DuplicateSearch::start(
            &report.tree,
            crate::pro::DUPLICATES_MIN_SIZE,
            self.threads,
        ) {
            Ok(search) => {
                self.duplicates = Some(search);
                self.duplicates_reported = false;
                self.reset_ranking(None);
            }
            Err(error) => {
                self.notice = Some(Notice::warning(format!("Couldn't compare files: {error}")));
            }
        }
    }

    /// Pro: whether the user started a search, and whether it's still running.
    #[cfg(feature = "pro")]
    pub(super) fn duplicate_search(&self) -> Option<bool> {
        self.duplicates
            .as_ref()
            .map(|search| search.found().is_none())
    }

    /// Pro: fills the Duplicates view's rows from a finished search, for the current
    /// scan, and the Explain panel once the system answers. Returns whether a search is
    /// running, so its progress needs drawing, and whether anything changed.
    #[cfg(feature = "pro")]
    pub(super) fn poll_views(&mut self) -> (bool, bool) {
        let mut changed = false;
        if let Some(lines) = self.explain_job.as_ref().and_then(|job| job.get()) {
            if matches!(self.panel, Some(Panel::Explain(_))) {
                self.panel = Some(Panel::Explain(lines.clone()));
            }
            self.explain_job = None;
            changed = true;
        }
        let Some(search) = &self.duplicates else {
            return (false, changed);
        };
        let Some(found) = search.found() else {
            return (true, changed);
        };
        let (Some(report), Mode::Duplicates) = (&self.report, self.mode) else {
            return (false, changed);
        };
        let tree = Arc::clone(&report.tree);
        if self.ranking.as_ref().is_some_and(|ranking| {
            ranking.key == self.rank_key() && Arc::ptr_eq(&ranking.tree, &tree)
        }) {
            return (false, changed);
        }
        let groups = found.groups_in(&tree);
        let unread = found.unread;
        let mut labels = std::collections::HashMap::new();
        let mut items = Vec::new();
        let mut extra = 0_u64;
        for group in &groups {
            let group_extra = group.extra_bytes(&tree);
            extra = extra.saturating_add(group_extra);
            for (index, &id) in group.files.iter().enumerate() {
                let mut label = if index == 0 {
                    format!(
                        "{} copies, {} extra",
                        group.files.len(),
                        format_bytes(group_extra)
                    )
                } else {
                    format!("copy {} of {}", index + 1, group.files.len())
                };
                if group.is_clone(index) {
                    label.push_str(", clone");
                }
                labels.insert(id, label);
                items.push(id);
            }
        }
        let preferred = self
            .selection_target
            .take()
            .or_else(|| self.current().map(Entry::path));
        self.duplicate_labels = labels;
        self.duplicate_summary = (groups.len(), extra);
        self.ranking = Some(Ranked {
            key: self.rank_key(),
            tree,
            folder: NodeId::ROOT,
            items,
        });
        self.rebuild(preferred.as_deref());
        if unread > 0 && !self.duplicates_reported {
            self.duplicates_reported = true;
            self.notice = Some(Notice::warning(format!(
                "{unread} {} couldn't be read to compare.",
                if unread == 1 { "file" } else { "files" }
            )));
        }
        (false, true)
    }

    /// Pro: explains the open folder's volume in a panel, asking the system in the
    /// background.
    #[cfg(feature = "pro")]
    fn open_explain(&mut self) {
        let Some(scope) = self.scope() else { return };
        let path = if self.mode == Mode::Folders {
            scope.path()
        } else {
            scope.tree().root_path().to_path_buf()
        };
        let job = Arc::new(std::sync::OnceLock::new());
        let answer = Arc::clone(&job);
        let spawned = std::thread::Builder::new()
            .name("diskuse16x-explain".into())
            .spawn(move || {
                let _ = answer.set(crate::pro::explain_lines(&path));
            });
        if let Err(error) = spawned {
            self.notice = Some(Notice::warning(format!(
                "Couldn't explain this volume: {error}"
            )));
            return;
        }
        self.explain_job = Some(job);
        self.open_panel(Panel::Explain(vec![
            "Asking the system about this volume…".into(),
        ]));
    }

    /// Pro: turns the Exclusive column on or off. It measures the finished scan in the
    /// background, so the walk itself stays as fast as before.
    #[cfg(feature = "pro")]
    fn toggle_exclusive(&mut self) {
        let Some(report) = &self.report else { return };
        if self.exclusive_column {
            self.exclusive_column = false;
            self.exclusive = None;
            return;
        }
        if self.imported {
            self.notice = Some(Notice::info(
                "Exclusive sizes need the files themselves. Scan the folder to see them.",
            ));
            return;
        }
        if report.is_preview && !self.is_active() {
            self.notice = Some(Notice::info(
                "Exclusive sizes need a finished scan. Press r to scan again.",
            ));
            return;
        }
        self.exclusive_column = true;
        self.sync_exclusive();
    }

    /// Pro: starts measuring the latest finished scan while the Exclusive column is on.
    #[cfg(feature = "pro")]
    fn sync_exclusive(&mut self) {
        if !self.exclusive_column || self.is_active() {
            return;
        }
        let Some(report) = self.report.as_ref().filter(|report| !report.is_preview) else {
            return;
        };
        if self
            .exclusive
            .as_ref()
            .is_some_and(|measurement| measurement.measures(&report.tree))
        {
            return;
        }
        match crate::pro::ExclusiveMeasurement::start(&report.tree, self.threads) {
            Ok(measurement) => {
                self.exclusive = Some(measurement);
                self.exclusive_drawn = false;
            }
            Err(error) => {
                self.exclusive_column = false;
                self.notice = Some(Notice::warning(format!(
                    "Couldn't measure exclusive sizes: {error}"
                )));
            }
        }
    }

    /// Pro: starts or restarts the measurement when the scan changed. Returns whether it
    /// is running, so the progress needs drawing, and whether it just finished.
    #[cfg(feature = "pro")]
    pub(super) fn poll_exclusive(&mut self) -> (bool, bool) {
        self.sync_exclusive();
        let Some(measurement) = &self.exclusive else {
            return (false, false);
        };
        let finished = measurement.result().is_some();
        let just_finished = finished && !self.exclusive_drawn;
        self.exclusive_drawn = finished;
        (!finished, just_finished)
    }

    /// Whether the rows show Pro's Exclusive column.
    pub(super) fn exclusive_column(&self) -> bool {
        #[cfg(feature = "pro")]
        return self.exclusive_column;
        #[cfg(not(feature = "pro"))]
        false
    }

    /// Pro: the Exclusive column's state for the rows: None when it's off, else the
    /// measurement's progress and, once finished for the rows' tree, its sizes.
    #[cfg(feature = "pro")]
    pub(super) fn exclusive_rows(&self) -> Option<(u8, Option<&crate::pro::Exclusive>)> {
        if !self.exclusive_column {
            return None;
        }
        let Some(measurement) = &self.exclusive else {
            return Some((0, None));
        };
        let current = self
            .ranking
            .as_ref()
            .is_some_and(|ranking| measurement.measures(&ranking.tree));
        Some((
            measurement.percent(),
            measurement.result().filter(|_| current),
        ))
    }

    /// Pro: the open scope's exclusive size, once measured.
    #[cfg(feature = "pro")]
    pub(super) fn exclusive_scope(&self) -> Option<u64> {
        let report = self.report.as_ref()?;
        let measurement = self.exclusive.as_ref().filter(|_| self.exclusive_column)?;
        if !measurement.measures(&report.tree) {
            return None;
        }
        Some(measurement.result()?.bytes(self.scope()?))
    }

    /// Re-ranks the current view, keeping the selection and the rows on screen meanwhile.
    fn rerank(&mut self) {
        if self.report.is_none() {
            return;
        }
        self.selection_target = self.current().map(Entry::path);
        self.requested_ranking = None;
    }

    fn cycle_sort(&mut self) {
        if self.mode == Mode::Folders {
            self.sort = self.sort.next();
            self.rerank();
        }
    }

    fn toggle_metric(&mut self) {
        self.metric = match self.metric {
            Metric::Allocated => Metric::Apparent,
            Metric::Apparent => Metric::Allocated,
        };
        self.rerank();
    }

    fn move_selection(&mut self, delta: isize) {
        self.selected = self
            .selected
            .saturating_add_signed(delta)
            .min(self.rows.len().saturating_sub(1));
        self.set_page_rows(self.page_rows);
    }

    /// Pro: lists the file types in the open folder, by the current size.
    #[cfg(feature = "pro")]
    fn open_types(&mut self) {
        if let Some(scope) = self.scope() {
            let types = crate::pro::type_breakdown(scope, self.metric);
            self.open_panel(Panel::Types(types));
        }
    }

    fn open_panel(&mut self, panel: Panel) {
        self.panel = Some(panel);
        self.panel_items = 0;
        self.panel_offset = 0;
    }

    pub(super) fn set_panel_page(&mut self, items: usize, page: usize) {
        self.panel_items = items;
        self.panel_page = page.max(1);
        self.panel_offset = self.panel_offset.min(items.saturating_sub(self.panel_page));
    }

    fn panel_key(&mut self, code: KeyCode) {
        let last = self.panel_items.saturating_sub(self.panel_page);
        match code {
            KeyCode::Esc | KeyCode::Backspace | KeyCode::Left => self.panel = None,
            KeyCode::Char('?') if matches!(self.panel, Some(Panel::Help)) => self.panel = None,
            KeyCode::Char('e' | 'i') if matches!(self.panel, Some(Panel::Issues)) => {
                self.panel = None
            }
            #[cfg(feature = "pro")]
            KeyCode::Char('t') if matches!(self.panel, Some(Panel::Types(_))) => self.panel = None,
            #[cfg(feature = "pro")]
            KeyCode::Char('E') if matches!(self.panel, Some(Panel::Explain(_))) => {
                self.panel = None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.panel_offset = self.panel_offset.saturating_sub(1)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.panel_offset = (self.panel_offset + 1).min(last)
            }
            KeyCode::PageUp => {
                self.panel_offset = self.panel_offset.saturating_sub(self.panel_page)
            }
            KeyCode::PageDown => {
                self.panel_offset = (self.panel_offset + self.panel_page).min(last)
            }
            KeyCode::Home => self.panel_offset = 0,
            KeyCode::End => self.panel_offset = last,
            _ => {}
        }
    }

    pub(super) fn paste(&mut self, text: &str) {
        if self.original_query.is_some() {
            let remaining = FILTER_LIMIT.saturating_sub(self.query.chars().count());
            self.query.extend(
                text.chars()
                    .filter(|character| !character.is_control())
                    .take(remaining),
            );
            self.refresh();
        }
    }

    fn filter_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Enter => {
                self.original_query = None;
            }
            KeyCode::Esc => {
                self.query = self.original_query.take().unwrap_or_default();
                self.refresh();
            }
            KeyCode::Backspace => {
                self.query.pop();
                self.refresh();
            }
            KeyCode::Char(character)
                if !character.is_control() && self.query.chars().count() < FILTER_LIMIT =>
            {
                self.query.push(character);
                self.refresh();
            }
            KeyCode::Up => self.move_selection(-1),
            KeyCode::Down => self.move_selection(1),
            _ => {}
        }
    }

    /// Marks or unmarks the selected item, then moves to the next row.
    fn toggle_mark(&mut self) {
        let Some(entry) = self.current() else {
            return;
        };
        let (path, state) = (entry.path(), entry.state());
        if self.read_only {
            self.notice = Some(Notice::warning(self.read_only_notice()));
            return;
        }
        if !self.marked.remove(&path) {
            if let Some(reason) = kept_reason(state, &path) {
                self.notice = Some(Notice::warning(reason));
                return;
            }
            self.marked.insert(path);
        }
        self.move_selection(1);
    }

    /// Total allocated bytes of the marked items, counting nested marks once.
    pub(super) fn marked_bytes(&self) -> u64 {
        self.trash_items(self.marked.iter())
            .iter()
            .map(|item| item.allocated_bytes)
            .sum()
    }

    /// Resolves paths to items in the current report. A path inside another chosen
    /// folder is left out, since moving the folder moves it too.
    fn trash_items<'a>(&self, paths: impl Iterator<Item = &'a PathBuf>) -> Vec<TrashItem> {
        let Some(report) = &self.report else {
            return Vec::new();
        };
        let mut items: Vec<TrashItem> = Vec::new();
        // Sorted paths put every folder before its contents.
        let paths: BTreeSet<_> = paths.collect();
        for path in paths {
            if path == report.tree.root_path()
                || items.iter().any(|item| path.starts_with(&item.path))
            {
                continue;
            }
            if let Some(id) = report.tree.find(path) {
                items.push(TrashItem::new(report.tree.entry(id)));
            }
        }
        items
    }

    /// Asks to move the marked items, or the selected one, to the Trash.
    fn plan_trash(&mut self) {
        if self.read_only {
            self.notice = Some(Notice::warning(self.read_only_notice()));
            return;
        }
        if self.is_active() {
            self.notice = Some(Notice::warning(
                "Let the scan finish, or press c to stop it, before moving items to the Trash.",
            ));
            return;
        }
        let current = self.current().map(Entry::path);
        let items = if self.marked.is_empty() {
            self.trash_items(current.iter())
        } else {
            self.trash_items(self.marked.iter())
        };
        if let Some(reason) = items
            .iter()
            .find_map(|item| kept_reason(item.state, &item.path))
        {
            self.notice = Some(Notice::warning(reason));
            return;
        }
        if !items.is_empty() {
            let bytes = items.iter().map(|item| item.allocated_bytes).sum();
            let warning = self.review_warning(&items);
            self.prompt = Some(TrashPlan {
                items,
                bytes,
                warning,
            });
        }
    }

    /// Pro: a warning when the items are or hold folders to review before removing, which
    /// can't be recreated, such as Xcode archives or simulator devices.
    #[cfg(feature = "pro")]
    fn review_warning(&self, items: &[TrashItem]) -> Option<String> {
        let tree = &self.report.as_ref()?.tree;
        let mut labels: Vec<&'static str> = Vec::new();
        for item in items
            .iter()
            .filter(|item| item.kind == EntryKind::Directory)
        {
            let Some(entry) = tree.find(&item.path).map(|id| tree.entry(id)) else {
                continue;
            };
            let candidates = std::iter::once(entry).chain(entry.descendants());
            for folder in candidates.filter(|folder| folder.kind() == EntryKind::Directory) {
                if let Some(rule) = folder
                    .parent()
                    .and_then(|parent| crate::pro::cleanup_rule(parent, folder))
                    .filter(|rule| rule.review.is_some())
                    && !labels.contains(&rule.label)
                {
                    labels.push(rule.label);
                }
            }
        }
        match labels.as_slice() {
            [] => None,
            [label] => Some(format!("{label} can't be recreated.")),
            _ => Some("Xcode archives and simulator devices can't be recreated.".into()),
        }
    }

    #[cfg(not(feature = "pro"))]
    fn review_warning(&self, _items: &[TrashItem]) -> Option<String> {
        None
    }

    fn read_only_notice(&self) -> &'static str {
        if self.imported {
            IMPORTED_NOTICE
        } else {
            READ_ONLY_NOTICE
        }
    }

    /// Scans the open folder again, or the whole scope in the largest files view.
    fn request_rescan(&mut self) -> Action {
        if self.imported {
            self.notice = Some(Notice::warning(IMPORTED_NOTICE));
            return Action::Stay;
        }
        if self.is_active() {
            self.notice = Some(Notice::warning(
                "Let the scan finish, or press c to stop it, before rescanning.",
            ));
            return Action::Stay;
        }
        let Some(report) = &self.report else {
            return Action::Stay;
        };
        let root = report.tree.root_path().to_path_buf();
        let folder = match (self.mode, self.folder) {
            (Mode::Folders, Some(folder)) => report.tree.path(folder),
            _ => root.clone(),
        };
        let previous = std::mem::replace(&mut self.status, Status::Scanning);
        self.rescan = Some((folder.clone(), previous));
        self.progress = None;
        Action::Rescan { root, folder }
    }

    /// Replaces the rescanned folder in place. A cancelled or failed rescan keeps the
    /// earlier results, except that a folder that no longer exists leaves them.
    fn finish_rescan(&mut self, folder: &Path, previous: Status, result: io::Result<Report>) {
        let name = quoted_name(folder);
        let Some(report) = &self.report else {
            self.status = previous;
            return;
        };
        let whole = folder == report.tree.root_path();
        let before = report
            .tree
            .find(folder)
            .map(|id| report.tree.entry(id).allocated_bytes());
        match result {
            Ok(subtree) => {
                let after = subtree.root().allocated_bytes();
                let Some(updated) = report.replacing(subtree) else {
                    self.status = previous;
                    self.notice = Some(Notice::warning(format!(
                        "{name} is no longer in the results."
                    )));
                    return;
                };
                self.marked.retain(|path| updated.tree.find(path).is_some());
                self.install(updated);
                #[cfg(feature = "pro")]
                {
                    self.duplicates = None;
                }
                // Rescanning one folder doesn't finish a cancelled or failed scan.
                self.status = if whole { Status::Complete } else { previous };
                self.notice = Some(Notice::info(match before {
                    Some(before) if before != after => format!(
                        "Rescanned {name}: {}, was {}.",
                        format_bytes(after),
                        format_bytes(before)
                    ),
                    _ => format!("Rescanned {name}: {}, unchanged.", format_bytes(after)),
                }));
            }
            Err(error)
                if error.kind() == io::ErrorKind::Interrupted
                    && matches!(self.status, Status::Cancelling) =>
            {
                self.status = previous;
                self.notice = Some(Notice::info("Rescan cancelled. The earlier results stay."));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound && !whole => {
                self.status = previous;
                if let Some(id) = report.tree.find(folder) {
                    let updated = Report {
                        tree: Arc::new(report.tree.without(&[id])),
                        issues: report
                            .issues
                            .iter()
                            .filter(|issue| !issue.path.starts_with(folder))
                            .cloned()
                            .collect(),
                        excluded_count: report.excluded_count,
                        is_preview: report.is_preview,
                    };
                    self.marked.retain(|path| !path.starts_with(folder));
                    self.install(updated);
                }
                self.notice = Some(Notice::warning(format!(
                    "{name} no longer exists, so it left the results."
                )));
            }
            Err(error) => {
                self.status = previous;
                self.notice = Some(Notice::warning(format!("Couldn't rescan {name}: {error}")));
            }
        }
    }

    /// Removes the moved items from the tree in place, so totals, rankings, and item
    /// counts update without a rescan, and reports the outcome in the footer.
    pub(super) fn finish_trash(&mut self, results: Vec<(TrashItem, io::Result<PathBuf>)>) {
        let Some(report) = &self.report else { return };
        let mut moved = Vec::new();
        let mut freed = 0;
        let mut failed = Vec::new();
        for (item, result) in results {
            match result {
                Ok(_) => {
                    freed += item.allocated_bytes;
                    moved.push(item);
                }
                Err(error) => failed.push((item, error)),
            }
        }
        if !moved.is_empty() {
            let removed: Vec<_> = moved
                .iter()
                .filter_map(|item| report.tree.find(&item.path))
                .collect();
            let gone = |path: &Path| moved.iter().any(|item| path.starts_with(&item.path));
            self.marked.retain(|path| !gone(path));
            let updated = Report {
                tree: Arc::new(report.tree.without(&removed)),
                issues: report
                    .issues
                    .iter()
                    .filter(|issue| !gone(&issue.path))
                    .cloned()
                    .collect(),
                excluded_count: report.excluded_count,
                is_preview: report.is_preview,
            };
            self.install(updated);
        }
        self.notice = Some(match (moved.as_slice(), failed.first()) {
            (moved, None) => Notice::info(format!(
                "Moved {} ({}) to the Trash. Space is freed when you empty the Trash.",
                match moved {
                    [item] => item.quoted_name(),
                    _ => format!("{} items", moved.len()),
                },
                format_bytes(freed)
            )),
            ([], Some((item, error))) => {
                Notice::warning(format!("{} wasn't moved: {error}", item.quoted_name()))
            }
            (moved, Some((item, error))) => Notice::warning(format!(
                "Moved {} of {} items. {} wasn't moved: {error}",
                moved.len(),
                moved.len() + failed.len(),
                item.quoted_name()
            )),
        });
    }

    pub(super) fn key(&mut self, key: KeyEvent) -> Action {
        self.notice = None;
        if let Some(plan) = self.prompt.take() {
            if matches!(key.code, KeyCode::Char('y' | 'Y'))
                && !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
            {
                return Action::Trash(plan.items);
            }
            self.notice = Some(Notice::info("Nothing was moved."));
            return Action::Stay;
        }
        if self.original_query.is_some()
            && key.code == KeyCode::Char('u')
            && key.modifiers.contains(KeyModifiers::CONTROL)
        {
            self.query.clear();
            self.refresh();
            return Action::Stay;
        }
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
        {
            return Action::Stay;
        }
        if self.original_query.is_some() {
            self.filter_key(key.code);
            return Action::Stay;
        }
        if key.code == KeyCode::Char('q') {
            return Action::Quit;
        }
        if matches!(key.code, KeyCode::Char('c' | 'x')) && matches!(self.status, Status::Scanning) {
            self.status = Status::Cancelling;
            return Action::CancelScan;
        }
        if self.panel.is_some() {
            self.panel_key(key.code);
            return Action::Stay;
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::PageUp => self.move_selection(-(self.page_rows as isize)),
            KeyCode::PageDown => self.move_selection(self.page_rows as isize),
            KeyCode::Home => self.move_selection(-(self.selected as isize)),
            KeyCode::End => self.move_selection(self.rows.len() as isize),
            // Pro: Enter starts comparing files in an empty Duplicates view.
            #[cfg(feature = "pro")]
            KeyCode::Enter
                if self.mode == Mode::Duplicates
                    && self.rows.is_empty()
                    && self.duplicate_search() != Some(true) =>
            {
                self.start_duplicates()
            }
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => self.activate(),
            KeyCode::Esc | KeyCode::Backspace | KeyCode::Left | KeyCode::Char('h') => self.back(),
            KeyCode::Char('/') => self.original_query = Some(self.query.clone()),
            KeyCode::Char('f') => self.toggle_largest(),
            KeyCode::Char('s') => self.cycle_sort(),
            KeyCode::Char('a') => self.toggle_metric(),
            KeyCode::Char(' ') => self.toggle_mark(),
            KeyCode::Char('d') | KeyCode::Delete => self.plan_trash(),
            KeyCode::Char('r') => return self.request_rescan(),
            KeyCode::Char('e' | 'i') => self.open_panel(Panel::Issues),
            KeyCode::Char('?') => self.open_panel(Panel::Help),
            #[cfg(feature = "pro")]
            KeyCode::Char('t') => self.open_types(),
            #[cfg(feature = "pro")]
            KeyCode::Char('u') => self.toggle_cleanup(),
            #[cfg(feature = "pro")]
            KeyCode::Char('m') => self.toggle_treemap(),
            #[cfg(feature = "pro")]
            KeyCode::Char('X') => self.toggle_exclusive(),
            #[cfg(feature = "pro")]
            KeyCode::Char('D') => self.toggle_duplicates(),
            #[cfg(feature = "pro")]
            KeyCode::Char('E') => self.open_explain(),
            _ => {}
        }
        Action::Stay
    }
}

/// Why an item can't be moved to the Trash, or None if it can.
fn kept_reason(state: EntryState, path: &Path) -> Option<String> {
    let reason = match state {
        EntryState::Local | EntryState::HardLinkAlias => return None,
        EntryState::CloudOnly => "is cloud-only. Remove it in Finder or its cloud app.",
        EntryState::Protected => "is protected, so DiskUse16x couldn't see all of it.",
        EntryState::Pending => "wasn't scanned, so its size is unknown.",
        EntryState::Incomplete => "had read errors, so its size is uncertain.",
    };
    Some(format!(
        "{} {reason} It stays where it is.",
        quoted_name(path)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::{EntryState, TreeBuilder};
    use crate::tui::ranker::rank;
    use std::os::unix::ffi::OsStrExt;

    /// A file or folder for a hand-built tree. Paths give the names, so tests read
    /// like the scans they stand for.
    #[derive(Clone)]
    enum Spec {
        File {
            path: &'static str,
            allocated: u64,
            apparent: u64,
            state: EntryState,
        },
        Folder {
            path: &'static str,
            state: EntryState,
            children: Vec<Spec>,
        },
    }

    impl Spec {
        fn with_state(self, new: EntryState) -> Self {
            match self {
                Spec::File {
                    path,
                    allocated,
                    apparent,
                    ..
                } => Spec::File {
                    path,
                    allocated,
                    apparent,
                    state: new,
                },
                Spec::Folder { path, children, .. } => Spec::Folder {
                    path,
                    state: new,
                    children,
                },
            }
        }

        fn add(&self, builder: &mut TreeBuilder) {
            let name = |path: &str| Path::new(path).file_name().unwrap().as_bytes().to_vec();
            match self {
                Spec::File {
                    path,
                    allocated,
                    apparent,
                    state,
                } => builder.file(&name(path), *allocated, *apparent, *state),
                Spec::Folder {
                    path,
                    state,
                    children,
                } => {
                    builder.open_folder(&name(path), *state);
                    for child in children {
                        child.add(builder);
                    }
                    builder.close_folder();
                }
            }
        }

        fn tree(&self) -> Arc<Tree> {
            let Spec::Folder {
                path,
                state,
                children,
            } = self
            else {
                panic!("a tree's root is a folder");
            };
            let mut builder = TreeBuilder::new(PathBuf::from(path), *state);
            for child in children {
                child.add(&mut builder);
            }
            Arc::new(builder.finish().0)
        }
    }

    fn file(path: &'static str, allocated_bytes: u64) -> Spec {
        sized(path, allocated_bytes, allocated_bytes)
    }

    fn sized(path: &'static str, allocated: u64, apparent: u64) -> Spec {
        Spec::File {
            path,
            allocated,
            apparent,
            state: EntryState::Local,
        }
    }

    fn folder(path: &'static str, children: Vec<Spec>) -> Spec {
        Spec::Folder {
            path,
            state: EntryState::Local,
            children,
        }
    }

    fn show(explorer: &mut Explorer, root: Spec) {
        explorer.install(Report {
            tree: root.tree(),
            issues: Vec::new(),
            excluded_count: 0,
            is_preview: true,
        });
        settle_ranking(explorer);
    }

    fn settle_ranking(explorer: &mut Explorer) {
        assert!(explorer.is_ranking());
        let request = explorer
            .ranking_request()
            .expect("a new snapshot should need ranking");
        assert!(explorer.accept_ranking(rank(request)));
        assert!(!explorer.is_ranking());
    }

    fn key(explorer: &mut Explorer, code: KeyCode) -> Action {
        explorer.key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn names(explorer: &Explorer) -> Vec<String> {
        explorer
            .rows
            .iter()
            .map(|&row| {
                let tree = explorer.rows_tree().unwrap();
                tree.entry(row).name().to_string_lossy().into_owned()
            })
            .collect()
    }

    /// A finished scan of big (500 B in two files), mid (100 B), and small (10 B).
    fn trash_fixture() -> Explorer {
        let mut explorer = Explorer::new(Path::new("/r"));
        let big = folder("/r/big", vec![file("/r/big/1", 300), file("/r/big/2", 200)]);
        show(
            &mut explorer,
            folder("/r", vec![big, file("/r/mid", 100), file("/r/small", 10)]),
        );
        explorer.status = Status::Complete;
        explorer
    }

    fn confirm(explorer: &mut Explorer) -> Vec<TrashItem> {
        match key(explorer, KeyCode::Char('y')) {
            Action::Trash(items) => items,
            _ => panic!("y should confirm the prompt"),
        }
    }

    fn moved(items: Vec<TrashItem>) -> Vec<(TrashItem, io::Result<PathBuf>)> {
        items
            .into_iter()
            .map(|item| {
                let trashed = Path::new("/Trash").join(item.path.file_name().unwrap());
                (item, Ok(trashed))
            })
            .collect()
    }

    fn notice(explorer: &Explorer) -> (&str, bool) {
        let notice = explorer.notice.as_ref().expect("a notice");
        (notice.text.as_str(), notice.warning)
    }

    fn rescan_request(explorer: &mut Explorer) -> (PathBuf, PathBuf) {
        match key(explorer, KeyCode::Char('r')) {
            Action::Rescan { root, folder } => (root, folder),
            _ => panic!("r should request a rescan"),
        }
    }

    fn subtree(root: Spec) -> io::Result<Report> {
        Ok(Report {
            tree: root.tree(),
            issues: Vec::new(),
            excluded_count: 0,
            is_preview: false,
        })
    }

    #[test]
    fn rescans_the_open_folder_in_place() {
        let mut explorer = trash_fixture();
        key(&mut explorer, KeyCode::Enter);
        settle_ranking(&mut explorer);
        assert_eq!(
            rescan_request(&mut explorer),
            ("/r".into(), "/r/big".into())
        );
        assert!(explorer.is_active() && explorer.is_rescanning());
        assert!(matches!(
            key(&mut explorer, KeyCode::Char('r')),
            Action::Stay
        ));
        assert!(notice(&explorer).1);

        let big = folder("/r/big", vec![file("/r/big/1", 300), file("/r/big/3", 900)]);
        explorer.finish(subtree(big));
        settle_ranking(&mut explorer);
        assert!(matches!(explorer.status, Status::Complete));
        assert!(!explorer.is_rescanning());
        assert_eq!(
            notice(&explorer),
            ("Rescanned “big”: 1 KB, was 500 B.", false)
        );
        assert_eq!(names(&explorer), ["3", "1"]);
        assert_eq!(
            explorer.report.as_ref().unwrap().root().allocated_bytes(),
            1_310
        );
    }

    #[test]
    fn a_cancelled_or_failed_rescan_keeps_the_results() {
        let mut explorer = trash_fixture();
        explorer.status = Status::Cancelled;
        assert_eq!(rescan_request(&mut explorer), ("/r".into(), "/r".into()));
        assert!(matches!(
            key(&mut explorer, KeyCode::Char('c')),
            Action::CancelScan
        ));
        explorer.finish(Err(io::Error::from(io::ErrorKind::Interrupted)));
        assert!(matches!(explorer.status, Status::Cancelled));
        assert_eq!(
            notice(&explorer),
            ("Rescan cancelled. The earlier results stay.", false)
        );
        assert_eq!(
            explorer.report.as_ref().unwrap().root().allocated_bytes(),
            610
        );

        key(&mut explorer, KeyCode::Enter);
        settle_ranking(&mut explorer);
        rescan_request(&mut explorer);
        explorer.finish(Err(io::Error::from(io::ErrorKind::NotFound)));
        assert!(matches!(explorer.status, Status::Cancelled));
        assert!(
            notice(&explorer)
                .0
                .ends_with("no longer exists, so it left the results.")
        );
        let root = explorer.report.as_ref().unwrap().root();
        assert_eq!((root.allocated_bytes(), root.children().count()), (110, 2));

        rescan_request(&mut explorer);
        explorer.finish(Err(io::Error::from(io::ErrorKind::PermissionDenied)));
        assert!(notice(&explorer).0.starts_with("Couldn't rescan “r”"));
    }

    #[test]
    fn marks_items_and_trashes_them_in_place() {
        let mut explorer = trash_fixture();
        assert_eq!(names(&explorer), ["big", "mid", "small"]);
        key(&mut explorer, KeyCode::Char(' '));
        assert_eq!(names(&explorer)[explorer.selected], "mid");
        key(&mut explorer, KeyCode::Down);
        key(&mut explorer, KeyCode::Char(' '));
        assert_eq!(explorer.marked_bytes(), 510);

        key(&mut explorer, KeyCode::Char('d'));
        let plan = explorer.prompt.as_ref().expect("a confirmation prompt");
        assert_eq!((plan.items.len(), plan.bytes), (2, 510));
        let items = confirm(&mut explorer);
        explorer.finish_trash(moved(items));
        settle_ranking(&mut explorer);

        assert_eq!(names(&explorer), ["mid"]);
        let root = explorer.report.as_ref().unwrap().root();
        assert_eq!((root.allocated_bytes(), root.item_count()), (100, 1));
        assert!(explorer.marked.is_empty());
        assert_eq!(
            notice(&explorer),
            (
                "Moved 2 items (510 B) to the Trash. Space is freed when you empty the Trash.",
                false
            )
        );
    }

    #[test]
    fn trashing_a_nested_file_updates_every_total() {
        let mut explorer = trash_fixture();
        key(&mut explorer, KeyCode::Enter);
        settle_ranking(&mut explorer);
        assert_eq!(names(&explorer), ["1", "2"]);
        key(&mut explorer, KeyCode::Char('d'));
        let items = confirm(&mut explorer);
        explorer.finish_trash(moved(items));
        settle_ranking(&mut explorer);

        assert_eq!(names(&explorer), ["2"]);
        assert_eq!(explorer.scope().unwrap().allocated_bytes(), 200);
        let root = explorer.report.as_ref().unwrap().root();
        let totals = (
            root.allocated_bytes(),
            root.apparent_bytes(),
            root.item_count(),
        );
        assert_eq!(totals, (310, 310, 4));
        assert!(
            notice(&explorer)
                .0
                .starts_with("Moved “1” (300 B) to the Trash.")
        );
    }

    #[test]
    fn nested_marks_count_once_and_failures_stay_marked() {
        let mut explorer = trash_fixture();
        key(&mut explorer, KeyCode::Char(' '));
        key(&mut explorer, KeyCode::Up);
        key(&mut explorer, KeyCode::Enter);
        settle_ranking(&mut explorer);
        key(&mut explorer, KeyCode::Char(' '));
        assert_eq!(explorer.marked.len(), 2);
        assert_eq!(explorer.marked_bytes(), 500);

        key(&mut explorer, KeyCode::Char('d'));
        let items = confirm(&mut explorer);
        assert_eq!(items.len(), 1, "moving big moves big/1 too");
        let denied = io::Error::new(io::ErrorKind::PermissionDenied, "Permission denied");
        explorer.finish_trash(vec![(items[0].clone(), Err(denied))]);
        assert_eq!(explorer.marked.len(), 2);
        assert_eq!(
            notice(&explorer),
            ("“big” wasn't moved: Permission denied", true)
        );
        assert_eq!(
            explorer.report.as_ref().unwrap().root().allocated_bytes(),
            610
        );
    }

    #[test]
    fn the_prompt_needs_y_and_trashing_waits_for_the_scan() {
        let mut explorer = trash_fixture();
        key(&mut explorer, KeyCode::Char('d'));
        assert!(explorer.prompt.is_some());
        // Any other key, even q, answers no rather than acting.
        assert!(matches!(
            key(&mut explorer, KeyCode::Char('q')),
            Action::Stay
        ));
        assert!(explorer.prompt.is_none());
        assert_eq!(notice(&explorer), ("Nothing was moved.", false));

        explorer.status = Status::Scanning;
        key(&mut explorer, KeyCode::Char('d'));
        assert!(explorer.prompt.is_none());
        assert!(notice(&explorer).0.starts_with("Let the scan finish"));
        explorer.status = Status::Complete;

        explorer.read_only = true;
        key(&mut explorer, KeyCode::Char(' '));
        assert!(explorer.marked.is_empty());
        key(&mut explorer, KeyCode::Delete);
        assert!(explorer.prompt.is_none());
        assert_eq!(notice(&explorer), (READ_ONLY_NOTICE, true));
    }

    #[test]
    fn imported_scans_are_read_only_and_never_rescanned() {
        let mut explorer = trash_fixture();
        explorer.imported = true;
        explorer.read_only = true;
        key(&mut explorer, KeyCode::Char(' '));
        assert!(explorer.marked.is_empty());
        assert_eq!(notice(&explorer), (IMPORTED_NOTICE, true));
        key(&mut explorer, KeyCode::Char('d'));
        assert!(explorer.prompt.is_none());
        assert!(matches!(
            key(&mut explorer, KeyCode::Char('r')),
            Action::Stay
        ));
        assert!(!explorer.is_rescanning());
        assert_eq!(notice(&explorer), (IMPORTED_NOTICE, true));
    }

    #[test]
    fn cloud_only_and_protected_items_stay_put() {
        let mut explorer = Explorer::new(Path::new("/r"));
        let cloud = file("/r/cloud", 0).with_state(EntryState::CloudOnly);
        let locked = folder("/r/locked", Vec::new()).with_state(EntryState::Protected);
        show(
            &mut explorer,
            folder("/r", vec![cloud, locked, file("/r/plain", 5)]),
        );
        explorer.status = Status::Complete;
        assert_eq!(names(&explorer), ["plain", "cloud", "locked"]);

        key(&mut explorer, KeyCode::Down);
        key(&mut explorer, KeyCode::Char(' '));
        assert!(explorer.marked.is_empty());
        assert_eq!(names(&explorer)[explorer.selected], "cloud");
        assert!(notice(&explorer).0.contains("is cloud-only"));

        key(&mut explorer, KeyCode::Down);
        key(&mut explorer, KeyCode::Char('d'));
        assert!(explorer.prompt.is_none());
        assert!(notice(&explorer).0.contains("is protected"));
    }

    #[cfg(feature = "pro")]
    #[test]
    fn t_lists_the_open_folders_file_types() {
        let mut explorer = Explorer::new(Path::new("/r"));
        let root = folder(
            "/r",
            vec![
                folder(
                    "/r/clips",
                    vec![file("/r/clips/a.mov", 90), file("/r/clips/b.MOV", 30)],
                ),
                file("/r/notes.txt", 5),
            ],
        );
        show(&mut explorer, root);
        key(&mut explorer, KeyCode::Char('t'));
        let Some(Panel::Types(types)) = &explorer.panel else {
            panic!("t opens the types panel");
        };
        let summary: Vec<_> = types
            .iter()
            .map(|kind| (kind.label(), kind.file_count))
            .collect();
        assert_eq!(summary, [(".mov".to_string(), 2), (".txt".to_string(), 1)]);
        key(&mut explorer, KeyCode::Char('t'));
        assert!(explorer.panel.is_none());

        key(&mut explorer, KeyCode::Enter);
        key(&mut explorer, KeyCode::Char('t'));
        let Some(Panel::Types(types)) = &explorer.panel else {
            panic!("t opens the types panel in a folder");
        };
        assert_eq!(types.len(), 1);
        assert_eq!(types[0].allocated_bytes, 120);
    }

    #[cfg(feature = "pro")]
    #[test]
    fn x_measures_exclusive_sizes_after_the_scan() {
        let mut explorer = Explorer::new(Path::new("/r"));
        let root = folder(
            "/r",
            vec![
                folder("/r/big", vec![file("/r/big/1", 300), file("/r/big/2", 200)]),
                file("/r/small", 10),
            ],
        );
        // Before the scan finishes, the column waits for it.
        show(&mut explorer, root.clone());
        key(&mut explorer, KeyCode::Char('X'));
        assert!(explorer.exclusive_column);
        assert_eq!(explorer.poll_exclusive(), (false, false));
        assert_eq!(
            explorer.exclusive_rows().map(|(_, sizes)| sizes.is_some()),
            Some(false)
        );
        explorer.finish(Ok(Report {
            tree: root.tree(),
            issues: Vec::new(),
            excluded_count: 0,
            is_preview: false,
        }));
        settle_ranking(&mut explorer);
        let mut finished = false;
        for _ in 0..200 {
            let (running, just_finished) = explorer.poll_exclusive();
            if just_finished {
                finished = true;
                break;
            }
            assert!(running);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(finished, "the measurement finishes");
        // These paths don't exist, so nothing counts as shared.
        assert_eq!(explorer.exclusive_scope(), Some(510));
        let (_, sizes) = explorer.exclusive_rows().unwrap();
        let big = explorer.current().unwrap();
        assert_eq!(sizes.unwrap().bytes(big), 500);
        assert_eq!(explorer.poll_exclusive(), (false, false));
        key(&mut explorer, KeyCode::Char('X'));
        assert!(explorer.exclusive_rows().is_none() && explorer.exclusive_scope().is_none());

        // An imported scan's files may be elsewhere.
        explorer.imported = true;
        key(&mut explorer, KeyCode::Char('X'));
        assert!(!explorer.exclusive_column);
        assert!(notice(&explorer).0.contains("Scan the folder"));
    }

    #[cfg(feature = "pro")]
    #[test]
    fn d_lists_duplicates_after_enter_and_follows_the_trash() {
        let root =
            std::env::temp_dir().join(format!("diskuse16x-tui-duplicates-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let same: Vec<u8> = (0..1_100_000_u32)
            .map(|value| (value % 251) as u8)
            .collect();
        for name in ["a.bin", "b.bin", "c.bin"] {
            std::fs::write(root.join(name), &same).unwrap();
        }
        std::fs::write(root.join("small.txt"), b"x").unwrap();
        let report = crate::scanner::scan(
            &root,
            crate::scanner::Options {
                excludes: &crate::scanner::Excludes::new(Vec::<String>::new()).unwrap(),
                threads: 2,
            },
            false,
            &std::sync::atomic::AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        let mut explorer = Explorer::new(&root);
        explorer.finish(Ok(report));
        settle_ranking(&mut explorer);

        // The view reads nothing until Enter starts the search.
        key(&mut explorer, KeyCode::Char('D'));
        assert!(explorer.mode == Mode::Duplicates && explorer.rows.is_empty());
        assert_eq!(explorer.duplicate_search(), None);
        assert!(explorer.ranking_request().is_none() && !explorer.is_ranking());
        key(&mut explorer, KeyCode::Enter);
        assert!(explorer.duplicate_search().is_some());
        for _ in 0..400 {
            if explorer.poll_views().1 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(names(&explorer), ["a.bin", "b.bin", "c.bin"]);
        assert_eq!(explorer.duplicate_summary.0, 1);
        let first = explorer.rows[0];
        assert!(explorer.duplicate_labels[&first].starts_with("3 copies, "));

        // Moving a copy to the Trash keeps the rest of the group.
        key(&mut explorer, KeyCode::Down);
        key(&mut explorer, KeyCode::Char('d'));
        let items = confirm(&mut explorer);
        explorer.finish_trash(moved(items));
        assert!(explorer.poll_views().1);
        assert_eq!(names(&explorer), ["a.bin", "c.bin"]);
        let first = explorer.rows[0];
        assert!(explorer.duplicate_labels[&first].starts_with("2 copies, "));
        // Enter on a copy opens its folder.
        key(&mut explorer, KeyCode::Enter);
        assert!(explorer.mode == Mode::Folders);
    }

    #[cfg(feature = "pro")]
    #[test]
    fn warns_before_trashing_folders_that_cant_be_recreated() {
        let mut explorer = Explorer::new(Path::new("/u"));
        let xcode = folder(
            "/u/Library/Developer/Xcode",
            vec![
                folder(
                    "/u/Library/Developer/Xcode/Archives",
                    vec![file("/u/Library/Developer/Xcode/Archives/App", 6_000_000)],
                ),
                folder(
                    "/u/Library/Developer/Xcode/DerivedData",
                    vec![file("/u/Library/Developer/Xcode/DerivedData/b", 2_000_000)],
                ),
            ],
        );
        let developer = folder("/u/Library/Developer", vec![xcode]);
        show(
            &mut explorer,
            folder("/u", vec![folder("/u/Library", vec![developer])]),
        );
        explorer.status = Status::Complete;
        key(&mut explorer, KeyCode::Char('u'));
        settle_ranking(&mut explorer);
        // Review folders are listed but never marked for you.
        assert_eq!(names(&explorer), ["Archives", "DerivedData"]);
        assert!(explorer.marked.is_empty());
        key(&mut explorer, KeyCode::Char('d'));
        let plan = explorer.prompt.take().expect("d asks to confirm");
        assert_eq!(
            plan.warning.as_deref(),
            Some("Xcode archives can't be recreated.")
        );
        key(&mut explorer, KeyCode::Down);
        key(&mut explorer, KeyCode::Char('d'));
        assert!(explorer.prompt.take().unwrap().warning.is_none());
        // A folder that holds archives warns too.
        key(&mut explorer, KeyCode::Char('u'));
        settle_ranking(&mut explorer);
        key(&mut explorer, KeyCode::Char('d'));
        assert!(explorer.prompt.take().unwrap().warning.is_some());
    }

    #[cfg(feature = "pro")]
    #[test]
    fn toggles_the_treemap_of_the_open_folder() {
        let mut explorer = trash_fixture();
        key(&mut explorer, KeyCode::Char('m'));
        assert!(explorer.treemap);
        // Moving and opening work as in the list.
        key(&mut explorer, KeyCode::Down);
        assert_eq!(names(&explorer)[explorer.selected], "mid");
        key(&mut explorer, KeyCode::Char('m'));
        assert!(!explorer.treemap);
        key(&mut explorer, KeyCode::Char('f'));
        key(&mut explorer, KeyCode::Char('m'));
        assert!(explorer.treemap && explorer.mode == Mode::Folders);
    }

    #[cfg(feature = "pro")]
    #[test]
    fn lists_cleanup_suggestions_for_the_trash() {
        let mut explorer = Explorer::new(Path::new("/r"));
        let web = folder(
            "/r/web",
            vec![
                folder(
                    "/r/web/node_modules",
                    vec![file("/r/web/node_modules/a.js", 2_000_000)],
                ),
                file("/r/web/index.js", 3_000_000),
            ],
        );
        let rust = folder(
            "/r/rust",
            vec![
                file("/r/rust/Cargo.toml", 10),
                folder(
                    "/r/rust/target",
                    vec![file("/r/rust/target/app", 4_000_000)],
                ),
            ],
        );
        show(&mut explorer, folder("/r", vec![web, rust]));
        explorer.status = Status::Complete;
        key(&mut explorer, KeyCode::Char('u'));
        settle_ranking(&mut explorer);
        assert!(explorer.mode == Mode::Cleanup);
        assert_eq!(names(&explorer), ["target", "node_modules"]);
        assert_eq!(
            explorer.row_path(explorer.current().unwrap()),
            Path::new("rust/target")
        );

        key(&mut explorer, KeyCode::Char(' '));
        key(&mut explorer, KeyCode::Char(' '));
        key(&mut explorer, KeyCode::Char('d'));
        let plan = explorer.prompt.as_ref().expect("d asks to confirm");
        assert_eq!(plan.bytes, 6_000_000);
        assert_eq!(confirm(&mut explorer).len(), 2);

        // Enter jumps to the suggestion in its folder, and u or Esc leaves the view.
        key(&mut explorer, KeyCode::Char('u'));
        key(&mut explorer, KeyCode::Char('u'));
        settle_ranking(&mut explorer);
        key(&mut explorer, KeyCode::Enter);
        settle_ranking(&mut explorer);
        assert!(explorer.mode == Mode::Folders);
        assert_eq!(names(&explorer)[explorer.selected], "target");
        key(&mut explorer, KeyCode::Char('u'));
        key(&mut explorer, KeyCode::Esc);
        assert!(explorer.mode == Mode::Folders);
    }

    #[test]
    fn cycles_sort_keys_and_keeps_the_selection() {
        let mut explorer = Explorer::new(Path::new("/r"));
        let three = vec![file("/r/b/1", 10), file("/r/b/2", 10), file("/r/b/3", 10)];
        let root = folder(
            "/r",
            vec![
                folder("/r/b", three),
                file("/r/C", 100),
                folder("/r/a", vec![file("/r/a/1", 50)]),
            ],
        );
        show(&mut explorer, root);
        assert_eq!(names(&explorer), ["C", "a", "b"]);
        key(&mut explorer, KeyCode::Down);
        assert_eq!(names(&explorer)[explorer.selected], "a");

        key(&mut explorer, KeyCode::Char('s'));
        settle_ranking(&mut explorer);
        assert_eq!(explorer.sort, Sort::Items);
        assert_eq!(names(&explorer), ["b", "a", "C"]);
        assert_eq!(names(&explorer)[explorer.selected], "a");

        key(&mut explorer, KeyCode::Char('s'));
        settle_ranking(&mut explorer);
        assert_eq!(names(&explorer), ["a", "b", "C"]);

        key(&mut explorer, KeyCode::Char('s'));
        settle_ranking(&mut explorer);
        assert_eq!(explorer.sort, Sort::Size);
        assert_eq!(names(&explorer), ["C", "a", "b"]);
    }

    #[test]
    fn toggles_apparent_sizes_in_both_views() {
        let mut explorer = Explorer::new(Path::new("/r"));
        let root = folder(
            "/r",
            vec![
                sized("/r/sparse", 4_096, 1_000_000),
                sized("/r/dense", 8_192, 8_000),
            ],
        );
        show(&mut explorer, root);
        assert_eq!(names(&explorer), ["dense", "sparse"]);

        key(&mut explorer, KeyCode::Char('a'));
        settle_ranking(&mut explorer);
        assert_eq!(explorer.metric, Metric::Apparent);
        assert_eq!(names(&explorer), ["sparse", "dense"]);

        key(&mut explorer, KeyCode::Char('f'));
        settle_ranking(&mut explorer);
        assert_eq!(names(&explorer), ["sparse", "dense"]);
        // Sorting applies to folders only; the largest files stay ranked by size.
        key(&mut explorer, KeyCode::Char('s'));
        assert_eq!(explorer.sort, Sort::Size);
    }

    #[test]
    fn selected_path_survives_live_reranking() {
        let mut explorer = Explorer::new(Path::new("/fixture"));
        show(
            &mut explorer,
            folder(
                "/fixture",
                vec![file("/fixture/a", 120), file("/fixture/b", 50)],
            ),
        );
        key(&mut explorer, KeyCode::Down);
        assert_eq!(explorer.current().unwrap().path(), Path::new("/fixture/b"));

        show(
            &mut explorer,
            folder(
                "/fixture",
                vec![file("/fixture/a", 120), file("/fixture/b", 900)],
            ),
        );
        assert_eq!(explorer.selected, 0);
        assert_eq!(explorer.current().unwrap().path(), Path::new("/fixture/b"));
    }

    #[test]
    fn largest_file_jump_selects_it_in_its_folder() {
        let mut explorer = Explorer::new(Path::new("/fixture"));
        show(
            &mut explorer,
            folder(
                "/fixture",
                vec![
                    file("/fixture/abc", 10),
                    folder(
                        "/fixture/nested one",
                        vec![file("/fixture/nested one/movie", 90_000)],
                    ),
                ],
            ),
        );

        key(&mut explorer, KeyCode::Char('f'));
        settle_ranking(&mut explorer);
        assert_eq!(
            explorer.current().unwrap().path(),
            Path::new("/fixture/nested one/movie")
        );
        key(&mut explorer, KeyCode::Enter);
        settle_ranking(&mut explorer);
        assert_eq!(
            explorer.scope().unwrap().path(),
            Path::new("/fixture/nested one")
        );
        assert_eq!(
            explorer.current().unwrap().path(),
            Path::new("/fixture/nested one/movie")
        );
        key(&mut explorer, KeyCode::Backspace);
        settle_ranking(&mut explorer);
        assert_eq!(
            explorer.current().unwrap().path(),
            Path::new("/fixture/nested one")
        );
    }

    #[test]
    fn filter_restores_on_escape_without_ranking_again() {
        let mut explorer = Explorer::new(Path::new("/fixture"));
        show(
            &mut explorer,
            folder(
                "/fixture",
                vec![file("/fixture/A", 120), file("/fixture/b", 50)],
            ),
        );
        key(&mut explorer, KeyCode::Char('/'));
        explorer.paste("b");
        assert_eq!(explorer.rows.len(), 1);
        assert_eq!(explorer.current().unwrap().path(), Path::new("/fixture/b"));
        assert!(explorer.ranking_request().is_none());
        key(&mut explorer, KeyCode::Esc);
        assert_eq!(explorer.rows.len(), 2);
        assert_eq!(explorer.current().unwrap().path(), Path::new("/fixture/b"));
    }

    #[test]
    fn intentional_cancellation_retains_preview() {
        let mut explorer = Explorer::new(Path::new("/fixture"));
        show(
            &mut explorer,
            folder("/fixture", vec![file("/fixture/a", 120)]),
        );
        assert!(matches!(
            key(&mut explorer, KeyCode::Char('c')),
            Action::CancelScan
        ));
        explorer.finish(Err(io::Error::new(io::ErrorKind::Interrupted, "Cancelled")));
        assert!(matches!(explorer.status, Status::Cancelled));
        assert_eq!(explorer.rows.len(), 1);
        assert!(explorer.quit().is_ok());
    }
}
