mod explorer;
mod ranker;
mod view;

use crate::scanner::{self, EntryKind, Event as ScanEvent, Excludes, Report};
use crossterm::{
    cursor::Show,
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEventKind,
        KeyModifiers,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use explorer::{Action, Explorer, TrashItem};
use ranker::RankWorker;
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{
    io::{self, IsTerminal},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub fn run(
    path: &Path,
    excludes: Excludes,
    threads: usize,
    plain: bool,
    read_only: bool,
) -> io::Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "explore requires an interactive terminal. Use space16x scan for reports and scripts.",
        ));
    }

    let _mode = TerminalMode::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let mut worker = ScanWorker::start(path.to_path_buf(), None, excludes.clone(), threads)?;
    let ranking = RankWorker::start()?;
    let mut explorer = Explorer::new(path);
    explorer.read_only = read_only;
    let started = Instant::now();
    let mut last_tick = 0;
    let mut dirty = true;

    loop {
        let worker_finished = worker.thread.is_finished();
        dirty |= explorer.apply(worker.take());
        if worker_finished && explorer.is_active() {
            explorer.finish(Err(io::Error::other(
                "The scan worker stopped unexpectedly.",
            )));
            dirty = true;
        }
        if let Some(result) = ranking.take() {
            dirty |= explorer.accept_ranking(result);
        }
        if let Some(request) = explorer.ranking_request() {
            ranking.submit(request)?;
        }
        let tick = started.elapsed().as_millis() / 150;
        if dirty || (explorer.is_active() && tick != last_tick) {
            terminal.draw(|frame| {
                view::draw(frame, &mut explorer, (tick % 4) as usize);
                if plain {
                    view::strip_colors(frame.buffer_mut());
                }
            })?;
            last_tick = tick;
            dirty = false;
        }
        if !event::poll(Duration::from_millis(75))? {
            continue;
        }
        match event::read()? {
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    return Err(io::Error::new(
                        io::ErrorKind::Interrupted,
                        "Explorer interrupted.",
                    ));
                }
                match explorer.key(key) {
                    Action::Stay => {}
                    Action::CancelScan => worker.cancel(),
                    Action::Trash(items) => {
                        let results = items
                            .into_iter()
                            .map(|item| {
                                let result = move_to_trash(&item);
                                (item, result)
                            })
                            .collect();
                        explorer.finish_trash(results);
                    }
                    Action::Rescan { root, folder } => {
                        worker = ScanWorker::start(folder, Some(root), excludes.clone(), threads)?;
                    }
                    Action::Quit => return explorer.quit(),
                }
                dirty = true;
            }
            Event::Paste(text) => {
                explorer.paste(&text);
                dirty = true;
            }
            Event::Resize(_, _) => dirty = true,
            _ => {}
        }
    }
}

/// Moves a scanned item to the Trash after checking that it is still the same kind of
/// item. The check doesn't follow a symbolic link, and the move never does either.
fn move_to_trash(item: &TrashItem) -> io::Result<PathBuf> {
    let metadata = std::fs::symlink_metadata(&item.path)?;
    if metadata.is_dir() != (item.kind == EntryKind::Directory) {
        return Err(io::Error::other("it changed since the scan"));
    }
    crate::trash::move_to_trash(&item.path)
}

struct TerminalMode;

impl TerminalMode {
    fn enter() -> io::Result<Self> {
        restore_terminal_before_panic_message();
        enable_raw_mode()?;
        let mode = Self;
        execute!(io::stdout(), EnterAlternateScreen, EnableBracketedPaste)?;
        Ok(mode)
    }
}

impl Drop for TerminalMode {
    fn drop(&mut self) {
        restore_terminal();
    }
}

fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(
        io::stdout(),
        DisableBracketedPaste,
        LeaveAlternateScreen,
        Show
    );
}

/// The default panic hook prints before unwinding runs `TerminalMode::drop`, so the
/// message would land on the alternate screen and vanish. Restore the terminal first
/// when the UI thread panics. Worker panics surface through the explorer instead.
fn restore_terminal_before_panic_message() {
    let ui_thread = thread::current().id();
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if thread::current().id() == ui_thread {
            restore_terminal();
        }
        previous(info);
    }));
}

struct Progress {
    visited: u64,
    path: PathBuf,
}

#[derive(Default)]
struct ScanUpdate {
    progress: Option<Progress>,
    preview: Option<Report>,
    finished: Option<io::Result<Report>>,
}

struct ScanWorker {
    cancelled: Arc<AtomicBool>,
    update: Arc<Mutex<ScanUpdate>>,
    thread: JoinHandle<()>,
}

impl ScanWorker {
    /// Scans `path`, or rescans that folder of an earlier scan of `root` without
    /// previews.
    fn start(
        path: PathBuf,
        root: Option<PathBuf>,
        excludes: Excludes,
        threads: usize,
    ) -> io::Result<Self> {
        let cancelled = Arc::new(AtomicBool::new(false));
        let update = Arc::new(Mutex::new(ScanUpdate::default()));
        let worker_cancelled = Arc::clone(&cancelled);
        let worker_update = Arc::clone(&update);
        let thread = thread::Builder::new()
            .name("space16x-scan".into())
            .spawn(move || {
                let notify = |event: ScanEvent| {
                    let mut pending = worker_update
                        .lock()
                        .unwrap_or_else(|error| error.into_inner());
                    let superseded = match event {
                        ScanEvent::Progress { visited, path } => {
                            pending.progress = Some(Progress { visited, path });
                            None
                        }
                        ScanEvent::Snapshot(report) if report.is_preview => {
                            pending.preview.replace(report)
                        }
                        ScanEvent::Snapshot(_) => None,
                    };
                    drop(pending);
                    drop(superseded);
                };
                let options = scanner::Options {
                    excludes: &excludes,
                    threads,
                };
                let result = match &root {
                    Some(root) => scanner::rescan(root, &path, options, &worker_cancelled, notify),
                    None => scanner::scan(&path, options, true, &worker_cancelled, notify),
                };
                let mut pending = worker_update
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                let superseded = if result.is_ok() {
                    pending.preview.take()
                } else {
                    None
                };
                pending.finished = Some(result);
                drop(pending);
                drop(superseded);
            })?;
        Ok(Self {
            cancelled,
            update,
            thread,
        })
    }

    fn take(&self) -> ScanUpdate {
        let mut pending = self
            .update
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        std::mem::take(&mut *pending)
    }

    fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

impl Drop for ScanWorker {
    fn drop(&mut self) {
        // A filesystem call can be waiting in the kernel. Let process exit restore the
        // user's prompt without waiting on a potentially uninterruptible metadata call.
        self.cancel();
    }
}
