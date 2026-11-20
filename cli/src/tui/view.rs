use super::explorer::{Explorer, LARGEST_LIMIT, Mode, Panel, Status};
use crate::{
    report::{format_binary_bytes, format_bytes, terminal_path, terminal_text},
    scanner::{Entry, EntryKind, EntryState},
};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Wrap},
};
use std::path::Path;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const ACCENT: Style = Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD);
const MUTED: Style = Style::new().add_modifier(Modifier::DIM);
const WARNING: Style = Style::new().fg(Color::Yellow);
const SELECTED: Style = Style::new()
    .fg(Color::Black)
    .bg(Color::Cyan)
    .add_modifier(Modifier::BOLD);

pub(super) fn draw(frame: &mut Frame<'_>, explorer: &mut Explorer, tick: usize) {
    let area = frame.area();
    if area.is_empty() {
        return;
    }
    draw_header(frame, line_at(area, 0), explorer, tick);
    if area.height == 1 {
        return;
    }
    draw_footer(frame, line_at(area, area.height - 1), explorer);
    if area.height < 4 {
        return;
    }

    let mut top = 1;
    if area.height >= 6 {
        draw_scope(frame, line_at(area, top), explorer);
        top += 1;
    }
    if area.height >= 9 {
        draw_summary(frame, line_at(area, top), explorer);
        top += 1;
    }
    if area.height >= 15
        && explorer.is_active()
        && let Some(progress) = &explorer.progress
    {
        labeled_path(frame, line_at(area, top), "Reading  ", &progress.path);
        top += 1;
    }

    let bottom = area.height
        - if area.height >= 8 && explorer.panel.is_none() {
            2
        } else {
            1
        };
    let body = Rect::new(area.x, area.y + top, area.width, bottom - top);
    if explorer.panel.is_some() {
        draw_panel(frame, body, explorer);
    } else {
        draw_results(frame, body, explorer);
    }

    if bottom + 1 < area.height
        && let Some(entry) = explorer.current()
    {
        labeled_path(frame, line_at(area, bottom), "Selected  ", &entry.path);
    }
}

fn draw_header(frame: &mut Frame<'_>, area: Rect, explorer: &Explorer, tick: usize) {
    let brand = if area.width < 44 {
        " 16X "
    } else {
        " SPACE 16X "
    };
    let status = match &explorer.status {
        Status::Scanning => {
            let spinner = ["◐", "◓", "◑", "◒"][tick];
            let activity = if let Some(progress) = &explorer.progress {
                format!("{spinner} Scanning  ·  {} visited", progress.visited)
            } else {
                format!("{spinner} Scanning")
            };
            (activity, ACCENT)
        }
        Status::Cancelling => ("Cancelling…".into(), WARNING),
        Status::Cancelled => ("Scan cancelled".into(), WARNING),
        Status::Failed(_) => ("Scan failed".into(), WARNING),
        Status::Complete => match issue_counts(explorer) {
            (0, 0) => ("Scan complete".into(), MUTED),
            (0, protected) => (
                format!(
                    "{protected} protected {} skipped",
                    if protected == 1 { "path" } else { "paths" }
                ),
                MUTED,
            ),
            (errors, protected) => {
                let mut status = format!(
                    "Partial scan  ·  {errors} {}",
                    if errors == 1 { "error" } else { "errors" }
                );
                if protected > 0 {
                    status.push_str(&format!("  ·  {protected} protected"));
                }
                (status, WARNING)
            }
        },
    };
    let mode = match (explorer.mode, area.width) {
        (Mode::Folders, 50..) => " Folders ",
        (Mode::Largest, 50..) => " Largest files ",
        (Mode::Largest, 28..=49) => " Largest ",
        _ => "",
    };
    let left_width = (brand.width() + mode.width()).min(usize::from(area.width));
    let left = Rect::new(area.x, area.y, left_width as u16, area.height);
    let right = Rect::new(
        left.right(),
        area.y,
        area.width.saturating_sub(left.width),
        area.height,
    );
    paint(
        frame,
        left,
        Line::from(vec![
            Span::styled(brand, SELECTED),
            Span::styled(mode, ACCENT),
        ]),
    );
    frame.render_widget(
        Paragraph::new(Line::styled(
            shorten(&status.0, right.width as usize, false),
            status.1,
        ))
        .alignment(Alignment::Right),
        right,
    );
}

fn draw_scope(frame: &mut Frame<'_>, area: Rect, explorer: &Explorer) {
    let Some(report) = &explorer.report else {
        labeled_path(frame, area, "Scope  ", &explorer.requested_path);
        return;
    };
    let path = explorer
        .scope()
        .map_or(report.root.path.as_path(), |entry| &entry.path);
    let mut crumbs = terminal_path(&report.root.path);
    if let Ok(child) = path.strip_prefix(&report.root.path) {
        for component in child.iter() {
            crumbs.push_str(" › ");
            crumbs.push_str(&terminal_path(Path::new(component)));
        }
    }
    paint(
        frame,
        area,
        Line::from(vec![
            Span::styled("Scope  ", MUTED),
            Span::raw(shorten(
                &crumbs,
                usize::from(area.width.saturating_sub(7)),
                true,
            )),
        ]),
    );
}

fn draw_summary(frame: &mut Frame<'_>, area: Rect, explorer: &Explorer) {
    if let Status::Failed(error) = &explorer.status {
        let error = error.to_string();
        let message = terminal_text(&error);
        paint(
            frame,
            area,
            Line::styled(shorten(&message, area.width as usize, false), WARNING),
        );
        return;
    }
    let Some(scope) = explorer.scope() else {
        return;
    };
    let (errors, protected) = issue_counts(explorer);
    let (qualifier, qualifier_style) = if matches!(explorer.status, Status::Cancelled) {
        ("  ·  cancelled preview", WARNING)
    } else if explorer
        .report
        .as_ref()
        .is_some_and(|report| report.is_preview)
    {
        ("  ·  provisional totals", WARNING)
    } else if errors > 0 {
        ("  ·  partial totals", WARNING)
    } else if protected > 0 {
        ("  ·  excludes protected paths", MUTED)
    } else {
        ("", MUTED)
    };
    let count = explorer.rows.len();
    let quantity = if explorer.is_ranking() {
        " · ranking…".into()
    } else {
        match explorer.mode {
            Mode::Folders if !explorer.query.is_empty() => format!(" · {count} matches"),
            Mode::Folders if count == 1 => " · 1 item".into(),
            Mode::Folders => format!(" · {count} items"),
            Mode::Largest => format!(" · {count} shown (up to {LARGEST_LIMIT})"),
        }
    };
    paint(
        frame,
        area,
        Line::from(vec![
            Span::styled(
                format!("{} allocated", format_bytes(scope.allocated_bytes)),
                ACCENT,
            ),
            Span::styled(qualifier, qualifier_style),
            Span::styled(quantity, MUTED),
            Span::styled(excluded_note(explorer), MUTED),
        ]),
    );
}

fn excluded_note(explorer: &Explorer) -> String {
    match explorer
        .report
        .as_ref()
        .map_or(0, |report| report.excluded_count)
    {
        0 => String::new(),
        count => format!(" · {count} excluded"),
    }
}

fn draw_results(frame: &mut Frame<'_>, area: Rect, explorer: &mut Explorer) {
    if area.is_empty() {
        return;
    }
    if explorer.rows.is_empty() {
        let empty = if explorer.is_ranking() {
            if explorer.mode == Mode::Largest {
                "Ranking largest files…"
            } else {
                "Ranking folder contents…"
            }
        } else if !explorer.query.is_empty() {
            "No matching items."
        } else if let Some(scope) = explorer.scope() {
            match scope.state {
                EntryState::CloudOnly => "Cloud-only folder. Local contents were not expanded.",
                EntryState::Pending if explorer.is_active() => {
                    "This folder is waiting to be scanned."
                }
                EntryState::Pending => "This folder was not scanned.",
                EntryState::Protected => {
                    "This folder is protected and can't be read. Press e for details."
                }
                EntryState::Incomplete => "No readable items here. Press e for scan issues.",
                _ if explorer.mode == Mode::Largest && explorer.is_active() => {
                    "Waiting for local files to rank…"
                }
                _ if explorer.mode == Mode::Largest => "No counted files to rank.",
                _ => "No scanned items here.",
            }
        } else if explorer.is_active() {
            "Reading the first two levels…"
        } else if matches!(explorer.status, Status::Cancelled) {
            "The scan was cancelled before a preview was available."
        } else {
            "Could not read the scan root."
        };
        explorer.set_page_rows(1);
        let widget = match &explorer.status {
            Status::Failed(error) => Paragraph::new(vec![
                Line::styled(empty, WARNING),
                Line::raw(terminal_text(&error.to_string()).into_owned()),
            ]),
            _ => Paragraph::new(Line::styled(empty, MUTED)),
        };
        frame.render_widget(widget.wrap(Wrap { trim: true }), area);
        return;
    }
    let columns = Columns::for_width(area.width);
    let header = columns.heading();
    paint(frame, line_at(area, 0), Line::styled(header, ACCENT));
    explorer.set_page_rows(usize::from(area.height.saturating_sub(1)));
    let total = explorer.scope().map_or(0, |scope| scope.allocated_bytes);
    let visible = explorer
        .rows
        .iter()
        .skip(explorer.offset)
        .take(usize::from(area.height - 1));
    for (index, entry) in visible.enumerate() {
        let selected = explorer.offset + index == explorer.selected;
        let row = line_at(area, index as u16 + 1);
        let path = terminal_path(explorer.row_path(entry));
        let line = columns.entry(entry, &path, total, selected);
        let style = if selected { SELECTED } else { Style::default() };
        frame.render_widget(Paragraph::new(line).style(style), row);
    }
}

struct Columns {
    name: usize,
    size: usize,
    state: usize,
    share: usize,
}

impl Columns {
    fn for_width(width: u16) -> Self {
        let width = usize::from(width);
        let size = (width / 3).min(13);
        let state = match width {
            82.. => 14,
            56..=81 => 12,
            _ => 0,
        };
        let share = match width {
            75.. => 16,
            48..=74 => 8,
            _ => 0,
        };
        Self {
            name: width - size - state - share,
            size,
            state,
            share,
        }
    }

    fn heading(&self) -> String {
        let state = if self.state == 0 {
            String::new()
        } else {
            padded("  State", self.state)
        };
        let share = if self.share == 0 {
            String::new()
        } else {
            format!("{:>width$}", "Share", width = self.share - 1) + " "
        };
        let size_label = if self.size < 9 { "Size" } else { "Allocated" };
        format!(
            "{}{:>width$}{state}{share}",
            padded("  Name", self.name),
            shorten(size_label, self.size, false),
            width = self.size,
        )
    }

    fn entry(&self, entry: &Entry, name: &str, total: u64, selected: bool) -> Line<'static> {
        let style = |value| if selected { SELECTED } else { value };
        let state = match entry.state {
            EntryState::Local => "",
            EntryState::Pending => "Pending",
            EntryState::CloudOnly => "Cloud only",
            EntryState::Protected => "Protected",
            EntryState::Incomplete => "Incomplete",
            EntryState::HardLinkAlias => "Hard link",
        };
        let state_style = match entry.state {
            EntryState::Pending | EntryState::Incomplete => WARNING,
            EntryState::CloudOnly => ACCENT,
            _ => MUTED,
        };
        let marker = if selected { "› " } else { "  " };
        let kind = match (entry.kind, entry.state, self.state) {
            (_, EntryState::CloudOnly, 0) => "○",
            (_, EntryState::Pending, 0) => "~",
            (_, EntryState::Protected, 0) => "×",
            (_, EntryState::Incomplete, 0) => "!",
            (_, EntryState::HardLinkAlias, 0) => "=",
            (EntryKind::Directory, _, _) => "▸",
            _ => "·",
        };
        let marker = shorten(marker, self.name, false);
        let kind = if self.name >= 6 {
            format!("{kind} ")
        } else {
            String::new()
        };
        let name_width = self.name.saturating_sub(marker.width() + kind.width());
        let amount = format_bytes(entry.allocated_bytes);
        let mut spans = vec![
            Span::styled(marker, style(ACCENT)),
            Span::styled(
                kind,
                style(if entry.kind == EntryKind::Directory {
                    ACCENT
                } else {
                    MUTED
                }),
            ),
            Span::raw(padded(name, name_width)),
            Span::raw(format!(
                "{:>width$}",
                shorten(&amount, self.size, false),
                width = self.size
            )),
        ];
        if self.state != 0 {
            spans.push(Span::styled(
                padded(&format!("  {state}"), self.state),
                style(state_style),
            ));
        }
        if self.share != 0 {
            let fraction = if total == 0 {
                0.0
            } else {
                (entry.allocated_bytes as f64 / total as f64).min(1.0)
            };
            if self.share >= 12 {
                let size = self.share - 9;
                let filled = (fraction * size as f64).round() as usize;
                spans.push(Span::raw("  "));
                spans.push(Span::styled("█".repeat(filled), style(ACCENT)));
                spans.push(Span::styled("░".repeat(size - filled), style(MUTED)));
            }
            let width = if self.share >= 12 { 6 } else { self.share - 1 };
            spans.push(Span::styled(
                format!("{:>width$.1}% ", 100.0 * fraction, width = width - 1),
                style(MUTED),
            ));
        }
        Line::from(spans)
    }
}

fn draw_footer(frame: &mut Frame<'_>, area: Rect, explorer: &Explorer) {
    if explorer.original_query.is_some() {
        let simple = "/ ";
        let suffix = "   Enter apply · Esc restore";
        let suffix = if area.width >= 46 { suffix } else { "" };
        let available = usize::from(area.width).saturating_sub(simple.width() + suffix.width() + 1);
        let input = terminal_text(&explorer.query);
        let query = shorten(&input, available, true);
        let cursor = (simple.width() + query.width()).min(usize::from(area.width - 1));
        paint(
            frame,
            area,
            Line::from(vec![
                Span::styled(simple, ACCENT),
                Span::raw(query),
                Span::styled(suffix, MUTED),
            ]),
        );
        frame.set_cursor_position((area.x + cursor as u16, area.y));
        return;
    }
    if !explorer.query.is_empty() && explorer.panel.is_none() {
        let query = terminal_text(&explorer.query);
        let prefix = "Filter ";
        let suffix = "  / edit · Esc clear · ? help";
        let budget = usize::from(area.width).saturating_sub(prefix.width() + suffix.width());
        let line = if budget > 0 {
            Line::from(vec![
                Span::styled(prefix, ACCENT),
                Span::raw(shorten(&query, budget, false)),
                Span::styled(suffix, MUTED),
            ])
        } else {
            Line::styled("/ edit · Esc clear", MUTED)
        };
        paint(frame, area, line);
        return;
    }
    let text = if explorer.panel.is_some() {
        " Esc back   ↑↓ scroll   q quit"
    } else if explorer.is_active() && area.width >= 72 {
        " ↑↓ move  ↵ open  ← back  / filter  f largest  e issues  c cancel  ? help"
    } else if area.width >= 65 {
        " ↑↓ move  ↵ open  ← back  / filter  f largest  e issues  ? help"
    } else if area.width >= 36 && explorer.is_active() {
        " ↑↓ move   ↵ open   c cancel   ? help"
    } else if area.width >= 36 {
        " ↑↓ move   ↵ open   / filter   ? help"
    } else {
        " ? help   q quit"
    };
    paint(frame, area, Line::styled(text, MUTED));
}

fn draw_panel(frame: &mut Frame<'_>, area: Rect, explorer: &mut Explorer) {
    let title = match explorer.panel {
        Some(Panel::Help) => " Help ",
        Some(Panel::Issues) => " Scan issues ",
        Some(Panel::Details) => " File details ",
        None => return,
    };
    let content = if area.width >= 24 && area.height >= 4 {
        let block = Block::bordered().title(Line::styled(title, ACCENT));
        let inner = block.inner(area);
        frame.render_widget(block, area);
        inner
    } else {
        paint(frame, line_at(area, 0), Line::styled(title, ACCENT));
        Rect::new(
            area.x,
            area.y + 1,
            area.width,
            area.height.saturating_sub(1),
        )
    };
    if content.is_empty() {
        return;
    }
    match explorer.panel {
        Some(Panel::Help) => draw_help(frame, content, explorer),
        Some(Panel::Issues) => draw_issues(frame, content, explorer),
        Some(Panel::Details) => draw_details(frame, content, explorer),
        None => {}
    }
}

fn draw_help(frame: &mut Frame<'_>, area: Rect, explorer: &mut Explorer) {
    let lines = [
        ("↑↓ / j k", "Move through the size ranking"),
        ("Page ↑↓", "Move one screen at a time"),
        ("Home / End", "Go to the first or last item"),
        ("Enter / →", "Open a folder or inspect a file"),
        ("Enter on largest", "Jump to the file’s folder"),
        ("← / Backspace", "Go to the parent folder"),
        ("/", "Filter displayed names or paths"),
        ("f", "Toggle largest files for the scan"),
        ("e / i", "Show skipped paths and errors"),
        ("c / x", "Cancel the scan and keep its preview"),
        ("Esc", "Go back or clear the filter"),
        ("q / Ctrl+C", "Quit and restore the terminal"),
        ("○ / ~", "Cloud only / pending"),
        ("× / !", "Protected / incomplete"),
        ("=", "Hard link"),
        ("16X Brains", "Read-only metadata"),
    ];
    explorer.set_panel_page(lines.len(), usize::from(area.height));
    let positions = if area.width >= 50 { 20 } else { 14 };
    for (y, (key, reason)) in lines
        .into_iter()
        .skip(explorer.panel_offset)
        .take(usize::from(area.height))
        .enumerate()
    {
        paint(
            frame,
            line_at(area, y as u16),
            Line::from(vec![
                Span::styled(padded(key, positions), ACCENT),
                Span::raw(reason),
            ]),
        );
    }
}

/// Counts scan errors and protected paths separately, since only errors warrant a warning.
fn issue_counts(explorer: &Explorer) -> (usize, usize) {
    explorer.report.as_ref().map_or((0, 0), |report| {
        let protected = report
            .issues
            .iter()
            .filter(|issue| issue.kind.is_protected())
            .count();
        (report.issues.len() - protected, protected)
    })
}

fn draw_issues(frame: &mut Frame<'_>, area: Rect, explorer: &mut Explorer) {
    let count = explorer
        .report
        .as_ref()
        .map_or(0, |report| report.issues.len());
    let item_height = if area.height >= 2 { 2 } else { 1 };
    explorer.set_panel_page(count, usize::from(area.height / item_height));
    if count == 0 {
        let message = match &explorer.status {
            Status::Failed(error) => terminal_text(&error.to_string()).into_owned(),
            Status::Scanning | Status::Cancelling => "No scan issues recorded so far.".into(),
            _ => "No scan issues recorded.".into(),
        };
        frame.render_widget(Paragraph::new(message).wrap(Wrap { trim: true }), area);
        return;
    }
    if let Some(report) = &explorer.report {
        // Errors come first. Protected paths follow in a muted style, since the OS refused them.
        let (protected, errors): (Vec<_>, Vec<_>) = report
            .issues
            .iter()
            .partition(|issue| issue.kind.is_protected());
        for (i, issue) in errors
            .into_iter()
            .chain(protected)
            .skip(explorer.panel_offset)
            .take(explorer.panel_page)
            .enumerate()
        {
            let y = i as u16 * item_height;
            let path = terminal_path(&issue.path);
            let style = if issue.kind.is_protected() {
                MUTED
            } else {
                WARNING
            };
            paint(
                frame,
                line_at(area, y),
                Line::styled(shorten(&path, area.width as usize, true), style),
            );
            if item_height > 1 {
                let message = terminal_text(&issue.message);
                let message = if issue.kind.is_protected() {
                    format!("Protected · {message}")
                } else {
                    message.into_owned()
                };
                paint(
                    frame,
                    line_at(area, y + 1),
                    Line::raw(shorten(&message, area.width as usize, false)),
                );
            }
        }
    }
}

fn draw_details(frame: &mut Frame<'_>, area: Rect, explorer: &mut Explorer) {
    explorer.set_panel_page(0, 1);
    let Some(entry) = explorer.current() else {
        return;
    };
    let state = match entry.state {
        EntryState::Local => "Local file",
        EntryState::Pending => "Pending",
        EntryState::CloudOnly => "Cloud-only reference",
        EntryState::Protected => "Protected: the OS refused access",
        EntryState::Incomplete => "Incomplete scan data",
        EntryState::HardLinkAlias => "Hard link counted at another path",
    };
    let bytes = entry.allocated_bytes;
    // The binary size adds detail only once it differs from the byte count.
    let binary = if bytes < 1024 {
        String::new()
    } else {
        format!("{} · ", format_binary_bytes(bytes))
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled(
                format!("{} allocated ({binary}{bytes} bytes)", format_bytes(bytes)),
                ACCENT,
            ),
            Line::styled(state, MUTED),
            Line::raw(""),
            Line::raw(terminal_path(&entry.path)),
        ])
        .wrap(Wrap { trim: false }),
        area,
    );
}

fn labeled_path(frame: &mut Frame<'_>, area: Rect, label: &'static str, path: &Path) {
    paint(
        frame,
        area,
        Line::from(vec![
            Span::styled(label, MUTED),
            Span::raw(shorten(
                &terminal_path(path),
                usize::from(area.width).saturating_sub(label.width()),
                true,
            )),
        ]),
    );
}

fn line_at(area: Rect, row: u16) -> Rect {
    Rect::new(area.x, area.y + row, area.width, 1)
}

fn paint(frame: &mut Frame<'_>, area: Rect, line: Line<'_>) {
    frame.render_widget(Paragraph::new(line), area);
}

fn padded(value: &str, width: usize) -> String {
    let mut content = shorten(value, width, false);
    content.extend(std::iter::repeat_n(
        ' ',
        width.saturating_sub(content.width()),
    ));
    content
}

fn shorten(value: &str, width: usize, keep_tail: bool) -> String {
    if value.width() <= width {
        return value.to_owned();
    }
    if width == 0 {
        return String::new();
    }
    let mut remaining = width - 1;
    let mut fit = |character: &char| {
        let cells = character.width().unwrap_or(0);
        if cells <= remaining {
            remaining -= cells;
            true
        } else {
            false
        }
    };
    if keep_tail {
        let tail: Vec<_> = value.chars().rev().take_while(&mut fit).collect();
        return std::iter::once('…').chain(tail.into_iter().rev()).collect();
    }
    value
        .chars()
        .take_while(&mut fit)
        .chain(std::iter::once('…'))
        .collect()
}
