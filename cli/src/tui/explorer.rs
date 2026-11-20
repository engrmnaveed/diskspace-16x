use super::ranker::{RankRequest, Ranked};
use super::{Progress, ScanUpdate};
use crate::{
    report::terminal_path,
    scanner::{Entry, EntryKind, Report},
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::{
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
}

pub(super) enum Action {
    Stay,
    CancelScan,
    Quit,
}

pub(super) struct Explorer {
    pub(super) requested_path: PathBuf,
    pub(super) report: Option<Report>,
    pub(super) status: Status,
    pub(super) progress: Option<Progress>,
    pub(super) mode: Mode,
    pub(super) rows: Vec<Arc<Entry>>,
    pub(super) selected: usize,
    pub(super) offset: usize,
    pub(super) page_rows: usize,
    pub(super) query: String,
    pub(super) original_query: Option<String>,
    pub(super) panel: Option<Panel>,
    pub(super) panel_offset: usize,
    pub(super) panel_items: usize,
    pub(super) panel_page: usize,
    folder: Option<Arc<Entry>>,
    requested_ranking: Option<(Mode, Weak<Entry>)>,
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
            folder: None,
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

    pub(super) fn finish(&mut self, result: io::Result<Report>) {
        self.status = match result {
            Ok(report) => {
                self.install(report);
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

    pub(super) fn scope(&self) -> Option<&Entry> {
        self.scope_reference().map(Arc::as_ref)
    }

    fn scope_reference(&self) -> Option<&Arc<Entry>> {
        let report = self.report.as_ref()?;
        match self.mode {
            Mode::Folders => self.folder.as_ref(),
            Mode::Largest => Some(&report.root),
        }
    }

    pub(super) fn ranking_request(&mut self) -> Option<RankRequest> {
        let source = Arc::clone(self.scope_reference()?);
        let snapshot = Arc::downgrade(&source);
        if self
            .requested_ranking
            .as_ref()
            .is_some_and(|(mode, requested)| {
                *mode == self.mode && Weak::ptr_eq(requested, &snapshot)
            })
        {
            return None;
        }
        self.requested_ranking = Some((self.mode, snapshot));
        Some(RankRequest {
            mode: self.mode,
            source,
        })
    }

    pub(super) fn is_ranking(&self) -> bool {
        self.scope_reference().is_some_and(|entry| {
            self.ranking
                .as_ref()
                .is_none_or(|rank| !Weak::ptr_eq(&rank.snapshot, &Arc::downgrade(entry)))
        })
    }

    pub(super) fn accept_ranking(&mut self, ranked: Ranked) -> bool {
        if ranked.mode != self.mode || self.scope().is_none_or(|scope| scope.path != ranked.folder)
        {
            return false;
        }
        let preferred = self
            .selection_target
            .clone()
            .or_else(|| self.current().map(|entry| entry.path.clone()));
        self.ranking = Some(ranked);
        self.rebuild(preferred.as_deref());
        if !self.is_ranking()
            || self
                .current()
                .is_some_and(|row| Some(&row.path) == self.selection_target.as_ref())
        {
            self.selection_target = None;
        }
        true
    }

    pub(super) fn current(&self) -> Option<&Entry> {
        self.rows.get(self.selected).map(Arc::as_ref)
    }

    pub(super) fn row_path<'a>(&self, entry: &'a Entry) -> &'a Path {
        match self.mode {
            Mode::Folders => entry.path.file_name().map(Path::new).unwrap_or(&entry.path),
            Mode::Largest => {
                let root = self
                    .report
                    .as_ref()
                    .map(|report| report.root.path.as_path());
                root.and_then(|root| entry.path.strip_prefix(root).ok())
                    .unwrap_or(&entry.path)
            }
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
        let old_folder = self.folder.as_ref().map(|entry| entry.path.as_path());
        let mut folder_path = old_folder.unwrap_or(&report.root.path).to_path_buf();
        let folder = loop {
            if let Some(folder) = find(&report.root, &folder_path) {
                break Arc::clone(folder);
            }
            if !folder_path.pop() || !folder_path.starts_with(&report.root.path) {
                break Arc::clone(&report.root);
            }
        };
        if self.mode == Mode::Folders && old_folder.is_some_and(|old| old != folder.path) {
            self.reset_ranking(None);
        }
        self.folder = Some(folder);
        self.report = Some(report);
    }

    fn rebuild(&mut self, preferred: Option<&Path>) {
        let items = self
            .ranking
            .as_ref()
            .map_or(&[][..], |ranking| &ranking.items);
        let rows: Vec<_> = if self.query.is_empty() {
            items.to_vec()
        } else {
            let needle = self.query.to_lowercase();
            items
                .iter()
                .filter(|entry| {
                    terminal_path(self.row_path(entry))
                        .to_lowercase()
                        .contains(&needle)
                })
                .cloned()
                .collect()
        };
        self.selected = preferred
            .and_then(|path| rows.iter().position(|entry| entry.path == path))
            .unwrap_or_else(|| self.selected.min(rows.len().saturating_sub(1)));
        self.rows = rows;
        self.set_page_rows(self.page_rows);
    }

    fn refresh(&mut self) {
        let previous = self.current().map(|entry| entry.path.clone());
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
            .and_then(|report| find(&report.root, path))
            .cloned()
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
        match (self.mode, &entry.kind) {
            (Mode::Folders, EntryKind::Directory) => self.switch_folder(&entry.path.clone(), None),
            (Mode::Folders, EntryKind::File) => self.open_panel(Panel::Details),
            (Mode::Largest, _) => {
                let path = entry.path.clone();
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
        } else if self.mode == Mode::Largest {
            self.toggle_largest();
        } else if let (Some(report), Some(folder)) = (&self.report, &self.folder)
            && folder.path != report.root.path
        {
            let child = folder.path.clone();
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
            Mode::Folders => Mode::Largest,
            Mode::Largest => Mode::Folders,
        };
        self.query.clear();
        self.reset_ranking(None);
    }

    fn move_selection(&mut self, delta: isize) {
        self.selected = self
            .selected
            .saturating_add_signed(delta)
            .min(self.rows.len().saturating_sub(1));
        self.set_page_rows(self.page_rows);
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

    pub(super) fn key(&mut self, key: KeyEvent) -> Action {
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
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => self.activate(),
            KeyCode::Esc | KeyCode::Backspace | KeyCode::Left | KeyCode::Char('h') => self.back(),
            KeyCode::Char('/') => self.original_query = Some(self.query.clone()),
            KeyCode::Char('f') => self.toggle_largest(),
            KeyCode::Char('e' | 'i') => self.open_panel(Panel::Issues),
            KeyCode::Char('?') => self.open_panel(Panel::Help),
            _ => {}
        }
        Action::Stay
    }
}

fn find<'a>(root: &'a Arc<Entry>, path: &Path) -> Option<&'a Arc<Entry>> {
    let relative = path.strip_prefix(&root.path).ok()?;
    let mut current = root;
    for name in relative.iter() {
        current = current
            .children
            .iter()
            .find(|child| child.path.file_name() == Some(name))?;
    }
    Some(current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::EntryState;
    use crate::tui::ranker::rank;

    fn file(path: &str, allocated_bytes: u64) -> Arc<Entry> {
        Arc::new(Entry {
            path: path.into(),
            allocated_bytes,
            apparent_bytes: allocated_bytes,
            item_count: 0,
            kind: EntryKind::File,
            state: EntryState::Local,
            children: Vec::new(),
        })
    }

    fn folder(path: &str, children: Vec<Arc<Entry>>) -> Arc<Entry> {
        Arc::new(Entry {
            path: path.into(),
            allocated_bytes: children.iter().map(|entry| entry.allocated_bytes).sum(),
            apparent_bytes: children.iter().map(|entry| entry.apparent_bytes).sum(),
            item_count: children.iter().map(|entry| entry.item_count + 1).sum(),
            kind: EntryKind::Directory,
            state: EntryState::Local,
            children,
        })
    }

    fn show(explorer: &mut Explorer, root: Arc<Entry>) {
        explorer.install(Report {
            root,
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
        assert_eq!(explorer.current().unwrap().path, Path::new("/fixture/b"));

        show(
            &mut explorer,
            folder(
                "/fixture",
                vec![file("/fixture/a", 120), file("/fixture/b", 900)],
            ),
        );
        assert_eq!(explorer.selected, 0);
        assert_eq!(explorer.current().unwrap().path, Path::new("/fixture/b"));
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
            explorer.current().unwrap().path,
            Path::new("/fixture/nested one/movie")
        );
        key(&mut explorer, KeyCode::Enter);
        settle_ranking(&mut explorer);
        assert_eq!(
            explorer.scope().unwrap().path,
            Path::new("/fixture/nested one")
        );
        assert_eq!(
            explorer.current().unwrap().path,
            Path::new("/fixture/nested one/movie")
        );
        key(&mut explorer, KeyCode::Backspace);
        settle_ranking(&mut explorer);
        assert_eq!(
            explorer.current().unwrap().path,
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
        assert_eq!(explorer.current().unwrap().path, Path::new("/fixture/b"));
        assert!(explorer.ranking_request().is_none());
        key(&mut explorer, KeyCode::Esc);
        assert_eq!(explorer.rows.len(), 2);
        assert_eq!(explorer.current().unwrap().path, Path::new("/fixture/b"));
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
