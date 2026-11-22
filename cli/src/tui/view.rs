use super::explorer::{Explorer, LARGEST_LIMIT, Mode, Panel, Sort, Status};
use crate::{
    report::{Metric, format_binary_bytes, format_bytes, terminal_path, terminal_text},
    scanner::{Entry, EntryKind, EntryState},
};
use ratatui::{
    Frame,
    buffer::Buffer,
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
        labeled_path(frame, line_at(area, bottom), "Selected  ", &entry.path());
    }
}

/// Removes colors for `NO_COLOR` and `--no-color`, keeping bold and dim emphasis.
/// Cells drawn on a background, such as the selection, switch to reverse video.
pub(super) fn strip_colors(buffer: &mut Buffer) {
    for cell in &mut buffer.content {
        if cell.bg != Color::Reset {
            cell.modifier.insert(Modifier::REVERSED);
        }
        cell.fg = Color::Reset;
        cell.bg = Color::Reset;
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
            let verb = if explorer.is_rescanning() {
                "Rescanning"
            } else {
                "Scanning"
            };
            let activity = if let Some(progress) = &explorer.progress {
                format!("{spinner} {verb}  ·  {} visited", progress.visited)
            } else {
                format!("{spinner} {verb}")
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
        #[cfg(feature = "pro")]
        (Mode::Cleanup, 28..) => " Cleanup ",
        #[cfg(feature = "pro")]
        (Mode::Duplicates, 28..) => " Duplicates ",
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
    let root = report.tree.root_path();
    let path = explorer
        .scope()
        .map_or_else(|| root.to_path_buf(), Entry::path);
    let mut crumbs = terminal_path(root);
    if let Ok(child) = path.strip_prefix(root) {
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
    let sorted = match (explorer.mode, explorer.sort) {
        (Mode::Folders, Sort::Items) => " · by items",
        (Mode::Folders, Sort::Name) => " · by name",
        _ => "",
    };
    let quantity = if explorer.is_ranking() {
        " · ranking…".into()
    } else {
        match explorer.mode {
            Mode::Folders if !explorer.query.is_empty() => format!(" · {count} matches"),
            Mode::Folders if count == 1 => " · 1 item".into(),
            Mode::Folders => format!(" · {count} items"),
            Mode::Largest => format!(" · {count} shown (up to {LARGEST_LIMIT})"),
            #[cfg(feature = "pro")]
            Mode::Cleanup if count == 1 => " · 1 suggestion".into(),
            #[cfg(feature = "pro")]
            Mode::Cleanup => format!(" · {count} suggestions"),
            #[cfg(feature = "pro")]
            Mode::Duplicates => match (explorer.duplicate_search(), explorer.duplicate_summary) {
                (Some(true), _) => " · comparing files…".into(),
                (Some(false), (1, extra)) => format!(" · 1 group, {} extra", format_bytes(extra)),
                (Some(false), (groups, extra)) => {
                    format!(" · {groups} groups, {} extra", format_bytes(extra))
                }
                (None, _) => " · files of 1 MB or more".into(),
            },
        }
    };
    paint(
        frame,
        area,
        Line::from(vec![
            Span::styled(
                format!(
                    "{} {}",
                    format_bytes(explorer.metric.bytes(scope)),
                    metric_word(explorer.metric)
                ),
                ACCENT,
            ),
            Span::styled(qualifier, qualifier_style),
            Span::styled(quantity, MUTED),
            Span::styled(sorted, MUTED),
            Span::styled(excluded_note(explorer), MUTED),
            Span::styled(exclusive_note(explorer), MUTED),
            Span::styled(marked_note(explorer), WARNING),
            Span::styled(
                if explorer.imported {
                    " · imported"
                } else if explorer.read_only {
                    " · read-only"
                } else {
                    ""
                },
                MUTED,
            ),
        ]),
    );
}

/// Pro: the open scope's exclusive size, or the measurement's progress.
#[cfg(feature = "pro")]
fn exclusive_note(explorer: &Explorer) -> String {
    match (explorer.exclusive_scope(), explorer.exclusive_rows()) {
        (Some(bytes), _) => format!(" · {} exclusive", format_bytes(bytes)),
        (None, Some((percent, None))) => format!(" · measuring exclusive {percent}%"),
        _ => String::new(),
    }
}

#[cfg(not(feature = "pro"))]
fn exclusive_note(_explorer: &Explorer) -> String {
    String::new()
}

fn marked_note(explorer: &Explorer) -> String {
    match explorer.marked.len() {
        0 => String::new(),
        count => format!(
            " · {count} marked ({})",
            format_bytes(explorer.marked_bytes())
        ),
    }
}

fn metric_word(metric: Metric) -> &'static str {
    match metric {
        Metric::Allocated => "allocated",
        Metric::Apparent => "apparent",
    }
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
        let empty = if let Some(message) = duplicates_message(explorer) {
            message
        } else if explorer.is_ranking() {
            if explorer.mode == Mode::Largest {
                "Ranking largest files…"
            } else if explorer.mode != Mode::Folders {
                "Finding cleanup suggestions…"
            } else {
                "Ranking folder contents…"
            }
        } else if !explorer.query.is_empty() {
            "No matching items."
        } else if let Some(scope) = explorer.scope() {
            match scope.state() {
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
                _ if explorer.mode != Mode::Folders && explorer.is_active() => {
                    "Cleanup suggestions appear when the scan finishes."
                }
                _ if explorer.mode != Mode::Folders => {
                    "No cleanup suggestions: no node_modules, build output or caches of 1 MB or more."
                }
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
    #[cfg(feature = "pro")]
    if explorer.treemap && explorer.mode == Mode::Folders {
        draw_treemap(frame, area, explorer);
        return;
    }
    // Pro shows modification dates when the scan read them, which an import doesn't.
    let dated = explorer
        .scope()
        .is_some_and(|scope| scope.details().modified().is_some());
    let columns = Columns::for_width(
        area.width,
        explorer.mode,
        explorer.sort,
        explorer.metric,
        dated,
        cfg!(feature = "pro") && explorer.exclusive_column(),
    );
    let header = columns.heading();
    paint(frame, line_at(area, 0), Line::styled(header, ACCENT));
    explorer.set_page_rows(usize::from(area.height.saturating_sub(1)));
    let total = explorer
        .scope()
        .map_or(0, |scope| explorer.metric.bytes(scope));
    let Some(tree) = explorer.rows_tree() else {
        return;
    };
    #[cfg(feature = "pro")]
    let exclusive = explorer.exclusive_rows();
    let visible = explorer
        .rows
        .iter()
        .skip(explorer.offset)
        .take(usize::from(area.height - 1));
    for (index, &id) in visible.enumerate() {
        let entry = tree.entry(id);
        let selected = explorer.offset + index == explorer.selected;
        let row = line_at(area, index as u16 + 1);
        #[cfg_attr(not(feature = "pro"), allow(unused_mut))]
        let mut path = terminal_path(&explorer.row_path(entry));
        #[cfg(feature = "pro")]
        if explorer.mode == Mode::Cleanup
            && let Some(rule) = entry
                .parent()
                .and_then(|parent| crate::pro::cleanup_rule(parent, entry))
        {
            path.push_str("  · ");
            path.push_str(rule.label);
            if rule.review.is_some() {
                path.push_str(", review: can't be recreated");
            }
        }
        #[cfg(feature = "pro")]
        if explorer.mode == Mode::Duplicates
            && let Some(label) = explorer.duplicate_labels.get(&id)
        {
            path.push_str("  · ");
            path.push_str(label);
        }
        let marked = explorer.marked.contains(&entry.path());
        // Pro: what trashing the row frees at least, once measured.
        #[cfg(feature = "pro")]
        let freed = exclusive.map(|(_, sizes)| {
            sizes.map_or_else(|| "…".into(), |sizes| format_bytes(sizes.bytes(entry)))
        });
        #[cfg(not(feature = "pro"))]
        let freed: Option<String> = None;
        let line = columns.entry(entry, &path, total, selected, marked, freed.as_deref());
        let style = if selected { SELECTED } else { Style::default() };
        frame.render_widget(Paragraph::new(line).style(style), row);
    }
}

/// Pro: what the empty Duplicates view says: how to start, progress, or no duplicates.
#[cfg(feature = "pro")]
fn duplicates_message(explorer: &Explorer) -> Option<&'static str> {
    if explorer.mode != Mode::Duplicates {
        return None;
    }
    Some(match explorer.duplicate_search() {
        Some(true) => "Comparing files of the same length…",
        Some(false) => "No duplicate files of 1 MB or more.",
        None if explorer.imported => {
            "Duplicates need the files themselves. Scan the folder to find them."
        }
        None if explorer.is_active() => "Duplicates can be found when the scan finishes.",
        None => {
            "Press Enter to compare files of 1 MB or more by contents. Only this view reads files."
        }
    })
}

#[cfg(not(feature = "pro"))]
fn duplicates_message(_explorer: &Explorer) -> Option<&'static str> {
    None
}

/// Pro: the open folder's contents as a squarified treemap, with the selection highlighted.
#[cfg(feature = "pro")]
fn draw_treemap(frame: &mut Frame<'_>, area: Rect, explorer: &mut Explorer) {
    use crate::pro::treemap;
    use ratatui::widgets::Borders;
    const COLORS: [Color; 5] = [
        Color::Cyan,
        Color::Green,
        Color::Blue,
        Color::Magenta,
        Color::Yellow,
    ];
    explorer.set_page_rows(usize::from(area.height));
    let Some(tree) = explorer.rows_tree() else {
        return;
    };
    let metric = explorer.metric;
    let selected = explorer.rows.get(explorer.selected).copied();
    let mut cells: Vec<_> = explorer
        .rows
        .iter()
        .map(|&id| tree.entry(id))
        .filter(|entry| metric.bytes(*entry) > 0)
        .collect();
    cells.sort_by_key(|entry| std::cmp::Reverse(metric.bytes(*entry)));
    cells.truncate(80);
    let sizes: Vec<u64> = cells.iter().map(|entry| metric.bytes(*entry)).collect();
    // Terminal cells are about twice as tall as wide, so lay out in half-rows.
    let bounds = treemap::Rect {
        x: 0.0,
        y: 0.0,
        width: f64::from(area.width),
        height: f64::from(area.height) * 2.0,
    };
    for (index, (entry, rect)) in cells
        .iter()
        .zip(treemap::layout(&sizes, bounds))
        .enumerate()
    {
        let (x0, y0) = (rect.x.round() as u16, (rect.y / 2.0).round() as u16);
        let (x1, y1) = (
            (rect.x + rect.width).round() as u16,
            ((rect.y + rect.height) / 2.0).round() as u16,
        );
        // A box needs room for its borders and part of a name.
        if x1.saturating_sub(x0) < 4 || y1.saturating_sub(y0) < 2 {
            continue;
        }
        let cell = Rect::new(area.x + x0, area.y + y0, x1 - x0, y1 - y0);
        let chosen = Some(entry.id()) == selected;
        let color = COLORS[index % COLORS.len()];
        let style = if chosen {
            SELECTED
        } else {
            Style::new().fg(color)
        };
        let width = usize::from(cell.width.saturating_sub(2));
        let name = terminal_path(&explorer.row_path(*entry));
        let suffix = if entry.kind() == EntryKind::Directory {
            "/"
        } else {
            ""
        };
        let block = Block::new()
            .borders(Borders::ALL)
            .border_style(style)
            .title(Span::styled(
                shorten(&format!("{name}{suffix}"), width, false),
                style,
            ));
        let inner = block.inner(cell);
        frame.render_widget(
            block.style(if chosen { style } else { Style::default() }),
            cell,
        );
        if inner.height > 0 {
            let size = shorten(&format_bytes(metric.bytes(*entry)), width, false);
            paint(
                frame,
                line_at(inner, 0),
                Line::styled(size, if chosen { style } else { MUTED }),
            );
        }
    }
}

struct Columns {
    name: usize,
    size: usize,
    items: usize,
    /// Pro's Modified column, which is zero-width in Basic.
    modified: usize,
    /// Pro's Exclusive column, shown when the user turns it on.
    exclusive: usize,
    state: usize,
    share: usize,
    /// The sorted column, marked in the heading: None in the largest-files view.
    sort: Option<Sort>,
    metric: Metric,
}

impl Columns {
    fn for_width(
        width: u16,
        mode: Mode,
        sort: Sort,
        metric: Metric,
        dated: bool,
        exclusive: bool,
    ) -> Self {
        let width = usize::from(width);
        let exclusive = match width {
            60.. if exclusive => 11,
            _ => 0,
        };
        let size = (width / 3).min(13);
        let items = match (mode, width) {
            (Mode::Folders, 70..) => 9,
            _ => 0,
        };
        let modified = match width {
            96.. if cfg!(feature = "pro") && dated => 12,
            _ => 0,
        };
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
            name: width - size - exclusive - items - modified - state - share,
            size,
            items,
            modified,
            exclusive,
            state,
            share,
            sort: (mode == Mode::Folders).then_some(sort),
            metric,
        }
    }

    fn heading(&self) -> String {
        let arrow = |sort: Sort| if self.sort == Some(sort) { " ↓" } else { "" };
        let name = if self.sort == Some(Sort::Name) {
            "  Name ↑"
        } else {
            "  Name"
        };
        let items = if self.items == 0 {
            String::new()
        } else {
            format!(
                "{:>width$}",
                format!("Items{}", arrow(Sort::Items)),
                width = self.items
            )
        };
        let modified = if self.modified == 0 {
            String::new()
        } else {
            format!("{:>width$}", "Modified", width = self.modified)
        };
        let exclusive = if self.exclusive == 0 {
            String::new()
        } else {
            format!("{:>width$}", "Exclusive", width = self.exclusive)
        };
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
        let size_label = match (self.size, self.metric) {
            (..9, _) => "Size",
            (_, Metric::Allocated) => "Allocated",
            (_, Metric::Apparent) => "Apparent",
        };
        let size_label = format!("{size_label}{}", arrow(Sort::Size));
        format!(
            "{}{:>width$}{exclusive}{items}{modified}{state}{share}",
            padded(name, self.name),
            shorten(&size_label, self.size, false),
            width = self.size,
        )
    }

    fn entry(
        &self,
        entry: Entry<'_>,
        name: &str,
        total: u64,
        selected: bool,
        marked: bool,
        freed: Option<&str>,
    ) -> Line<'static> {
        let style = |value| if selected { SELECTED } else { value };
        let state = match entry.state() {
            EntryState::Local => "",
            EntryState::Pending => "Pending",
            EntryState::CloudOnly => "Cloud only",
            EntryState::Protected => "Protected",
            EntryState::Incomplete => "Incomplete",
            EntryState::HardLinkAlias => "Hard link",
        };
        let state_style = match entry.state() {
            EntryState::Pending | EntryState::Incomplete => WARNING,
            EntryState::CloudOnly => ACCENT,
            _ => MUTED,
        };
        let marker = match (selected, marked) {
            (true, true) => "›✓",
            (true, false) => "› ",
            (false, true) => " ✓",
            (false, false) => "  ",
        };
        let kind = match (entry.kind(), entry.state(), self.state) {
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
        let bytes = self.metric.bytes(entry);
        let amount = format_bytes(bytes);
        let mut spans = vec![
            Span::styled(marker, style(if marked { WARNING } else { ACCENT })),
            Span::styled(
                kind,
                style(if entry.kind() == EntryKind::Directory {
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
        if self.exclusive != 0 {
            spans.push(Span::raw(format!(
                "{:>width$}",
                shorten(freed.unwrap_or(""), self.exclusive - 1, false),
                width = self.exclusive
            )));
        }
        if self.items != 0 {
            let count = if entry.kind() == EntryKind::Directory {
                format_count(entry.item_count())
            } else {
                String::new()
            };
            spans.push(Span::styled(
                format!(
                    "{:>width$}",
                    shorten(&count, self.items - 1, false),
                    width = self.items
                ),
                style(MUTED),
            ));
        }
        if self.modified != 0 {
            spans.push(Span::styled(
                format!("{:>width$}", modified_date(entry), width = self.modified),
                style(MUTED),
            ));
        }
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
                (bytes as f64 / total as f64).min(1.0)
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

/// Pro: the entry's modification date, or a folder's newest one below it.
#[cfg(feature = "pro")]
fn modified_date(entry: Entry<'_>) -> String {
    entry
        .details()
        .modified()
        .map_or_else(|| "-".into(), crate::pro::format_date)
}

#[cfg(not(feature = "pro"))]
fn modified_date(_entry: Entry<'_>) -> String {
    String::new()
}

/// Formats an item count with thousands separators, such as 12,345.
fn format_count(count: u64) -> String {
    let digits = count.to_string();
    let mut formatted = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            formatted.push(',');
        }
        formatted.push(digit);
    }
    formatted
}

fn draw_footer(frame: &mut Frame<'_>, area: Rect, explorer: &Explorer) {
    if let Some(plan) = &explorer.prompt {
        let what = match plan.items.as_slice() {
            [item] => item.quoted_name(),
            items => format!("{} items", items.len()),
        };
        let mut question = format!(" Move {what} ({}) to the Trash? ", format_bytes(plan.bytes));
        if let Some(warning) = &plan.warning {
            question.push_str(warning);
            question.push(' ');
        }
        let answer = "y/N";
        let budget = usize::from(area.width).saturating_sub(answer.width());
        paint(
            frame,
            area,
            Line::from(vec![
                Span::styled(shorten(&question, budget, false), WARNING),
                Span::styled(answer, ACCENT),
            ]),
        );
        return;
    }
    if let Some(notice) = &explorer.notice {
        let style = if notice.warning { WARNING } else { ACCENT };
        let text = format!(" {}", terminal_text(&notice.text));
        paint(
            frame,
            area,
            Line::styled(shorten(&text, usize::from(area.width), false), style),
        );
        return;
    }
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
    } else if !explorer.is_active() && !explorer.read_only && area.width >= 108 {
        " ↑↓ move  ↵ open  ← back  / filter  f largest  s sort  a size  ␣ mark  d trash  r rescan  e issues  ? help"
    } else if !explorer.is_active() && !explorer.read_only && area.width >= 98 {
        " ↑↓ move  ↵ open  ← back  / filter  f largest  s sort  a size  ␣ mark  d trash  e issues  ? help"
    } else if explorer.is_active() && area.width >= 90 {
        " ↑↓ move  ↵ open  ← back  / filter  f largest  s sort  a size  e issues  c cancel  ? help"
    } else if area.width >= 80 {
        " ↑↓ move  ↵ open  ← back  / filter  f largest  s sort  a size  e issues  ? help"
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
        #[cfg(feature = "pro")]
        Some(Panel::Types(_)) => " File types ",
        #[cfg(feature = "pro")]
        Some(Panel::Explain(_)) => " Explain disk space ",
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
        #[cfg(feature = "pro")]
        Some(Panel::Types(_)) => draw_types(frame, content, explorer),
        #[cfg(feature = "pro")]
        Some(Panel::Explain(_)) => draw_explain(frame, content, explorer),
        None => {}
    }
}

/// Pro: the open folder's volume and the space a scan can't see there, as `explain` prints.
#[cfg(feature = "pro")]
fn draw_explain(frame: &mut Frame<'_>, area: Rect, explorer: &mut Explorer) {
    let lines = match &explorer.panel {
        Some(Panel::Explain(lines)) => lines.clone(),
        _ => return,
    };
    explorer.set_panel_page(lines.len(), usize::from(area.height));
    for (y, line) in lines
        .iter()
        .skip(explorer.panel_offset)
        .take(usize::from(area.height))
        .enumerate()
    {
        let text = terminal_text(line);
        paint(
            frame,
            line_at(area, y as u16),
            Line::raw(shorten(&text, usize::from(area.width), false)),
        );
    }
}

/// Pro: the open folder's file types with their size, file count and share.
#[cfg(feature = "pro")]
fn draw_types(frame: &mut Frame<'_>, area: Rect, explorer: &mut Explorer) {
    let count = match &explorer.panel {
        Some(Panel::Types(types)) => types.len(),
        _ => return,
    };
    let rows = usize::from(area.height.saturating_sub(1));
    explorer.set_panel_page(count, rows);
    let total = explorer
        .scope()
        .map_or(0, |scope| explorer.metric.bytes(scope));
    let Some(Panel::Types(types)) = &explorer.panel else {
        return;
    };
    if types.is_empty() {
        paint(
            frame,
            line_at(area, 0),
            Line::styled("No files here.", MUTED),
        );
        return;
    }
    let heading = format!("{:>10} {:>10} {:>7}  Type", "Size", "Files", "Share");
    paint(frame, line_at(area, 0), Line::styled(heading, ACCENT));
    for (row, kind) in types
        .iter()
        .skip(explorer.panel_offset)
        .take(rows)
        .enumerate()
    {
        let bytes = match explorer.metric {
            Metric::Allocated => kind.allocated_bytes,
            Metric::Apparent => kind.apparent_bytes,
        };
        let share = if total == 0 {
            0.0
        } else {
            (100.0 * bytes as f64 / total as f64).min(100.0)
        };
        paint(
            frame,
            line_at(area, row as u16 + 1),
            Line::from(vec![
                Span::raw(format!(
                    "{:>10} {:>10} {share:>6.1}%  ",
                    format_bytes(bytes),
                    format_count(kind.file_count)
                )),
                Span::styled(kind.label(), ACCENT),
            ]),
        );
    }
}

fn draw_help(frame: &mut Frame<'_>, area: Rect, explorer: &mut Explorer) {
    let trash = if explorer.imported {
        [
            ("Imported", "Saved scans are read-only"),
            ("", "Scan the folder to rescan or trash"),
        ]
    } else if explorer.read_only {
        [
            ("Read-only", "Move to Trash is off"),
            ("", "Unset SPACE16X_READ_ONLY or --read-only"),
        ]
    } else {
        [
            ("Space", "Mark or unmark an item for the Trash"),
            ("d / Delete", "Move marked items, or the selected one"),
        ]
    };
    let lines = [
        ("↑↓ / j k", "Move through the size ranking"),
        ("Page ↑↓", "Move one screen at a time"),
        ("Home / End", "Go to the first or last item"),
        ("Enter / →", "Open a folder or inspect a file"),
        ("Enter on largest", "Jump to the file’s folder"),
        ("← / Backspace", "Go to the parent folder"),
        ("/", "Filter displayed names or paths"),
        ("f", "Toggle largest files for the scan"),
        ("s", "Sort folders by size, items, or name"),
        ("a", "Switch between allocated and apparent sizes"),
        #[cfg(feature = "pro")]
        ("t", "Show file types in the open folder"),
        #[cfg(feature = "pro")]
        ("u", "List folders tools recreate, to trash them"),
        #[cfg(feature = "pro")]
        ("m", "Show the open folder as a treemap"),
        #[cfg(feature = "pro")]
        ("X", "Show what trashing each item frees at least"),
        #[cfg(feature = "pro")]
        ("D", "List duplicate files; Enter compares them"),
        #[cfg(feature = "pro")]
        ("E", "Explain the open folder's volume"),
        trash[0],
        trash[1],
        ("r", "Rescan the open folder, or all in largest"),
        ("e / i", "Show skipped paths and errors"),
        ("c / x", "Cancel the scan and keep its preview"),
        ("Esc", "Go back or clear the filter"),
        ("q / Ctrl+C", "Quit and restore the terminal"),
        ("○ / ~", "Cloud only / pending"),
        ("× / !", "Protected / incomplete"),
        ("=", "Hard link"),
        ("✓", "Marked for the Trash"),
        ("16X Brains", "Scans read metadata only"),
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
    let state = match entry.state() {
        EntryState::Local => "Local file",
        EntryState::Pending => "Pending",
        EntryState::CloudOnly => "Cloud-only reference",
        EntryState::Protected => "Protected: the OS refused access",
        EntryState::Incomplete => "Incomplete scan data",
        EntryState::HardLinkAlias => "Hard link counted at another path",
    };
    let size_line = |bytes: u64, word: &str| {
        // The binary size adds detail only once it differs from the byte count.
        let binary = if bytes < 1024 {
            String::new()
        } else {
            format!("{} · ", format_binary_bytes(bytes))
        };
        format!("{} {word} ({binary}{bytes} bytes)", format_bytes(bytes))
    };
    let mut lines = vec![
        Line::styled(size_line(entry.allocated_bytes(), "allocated"), ACCENT),
        Line::raw(size_line(entry.apparent_bytes(), "apparent")),
    ];
    if cfg!(feature = "pro") && entry.details().modified().is_some() {
        lines.push(Line::raw(format!("Modified {}", modified_date(entry))));
    }
    lines.extend([
        Line::styled(state, MUTED),
        Line::raw(""),
        Line::raw(terminal_path(&entry.path())),
    ]);
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
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
