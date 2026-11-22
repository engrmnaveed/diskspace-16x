//! `scan --html FILE`: one self-contained HTML page to share a scan. It has a summary, a
//! treemap of the largest items, the scan root's contents, the largest files, file types
//! and, on shared servers, owners. The page has no scripts and loads nothing, and it
//! follows the reader's light or dark appearance.

use std::fmt::Write as _;
use std::io::{self, Write};
use std::time::{SystemTime, UNIX_EPOCH};

use super::modified::format_time;
use super::owners;
use super::treemap::{self, Rect};
use crate::report::{Metric, format_bytes, largest_files};
use crate::scanner::{Entry, EntryKind, Report};

/// The most items the treemap draws. The rest share one cell.
const MAP_ITEMS: usize = 30;
/// The most rows each table lists.
const TABLE_ROWS: usize = 50;
const TOP_FILES: usize = 25;
const TYPES: usize = 12;

const STYLE: &str = "\
:root{color-scheme:light dark;--bg:#fbfbfa;--fg:#1d2321;--muted:#66706c;--line:#dde3e0;\
--accent:#0f8f78;--folder:#2fae93;--file:#7a8b86;--other:#b9c4c0}\
@media (prefers-color-scheme:dark){:root{--bg:#161a19;--fg:#e8eeeb;--muted:#9aa6a1;\
--line:#2c3431;--accent:#4fd1b3;--folder:#1f8f78;--file:#56645f;--other:#3a4541}}\
*{box-sizing:border-box}body{margin:0;padding:32px 20px;background:var(--bg);color:var(--fg);\
font:15px/1.45 -apple-system,BlinkMacSystemFont,\"Segoe UI\",Roboto,sans-serif}\
main{max-width:1040px;margin:0 auto}h1{font-size:26px;margin:0 0 4px;word-break:break-all}\
h2{font-size:18px;margin:32px 0 10px}p{margin:4px 0;color:var(--muted)}\
.note{color:#b7791f}.map{position:relative;height:420px;border-radius:10px;overflow:hidden;\
background:var(--line)}.cell{position:absolute;border:1px solid var(--bg);overflow:hidden;\
padding:4px 6px;color:#fff;font-size:12px;line-height:1.3}.cell b{display:block;\
white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.folder{background:var(--folder)}\
.file{background:var(--file)}.other{background:var(--other);color:var(--fg)}\
table{width:100%;border-collapse:collapse;font-variant-numeric:tabular-nums}\
th,td{text-align:left;padding:6px 8px;border-bottom:1px solid var(--line);vertical-align:top}\
th{color:var(--muted);font-weight:600}td.n,th.n{text-align:right;white-space:nowrap}\
td.p{word-break:break-all}.bar{height:6px;border-radius:3px;background:var(--accent)}\
footer{margin-top:36px;color:var(--muted);font-size:13px}";

/// Writes the page for `report`, ranked by `metric`.
pub fn write(
    out: impl Write,
    report: &Report,
    metric: Metric,
    excludes: &[String],
    older_than: Option<u32>,
) -> io::Result<()> {
    let mut out = io::BufWriter::new(out);
    out.write_all(page(report, metric, excludes, older_than).as_bytes())?;
    out.flush()
}

fn page(report: &Report, metric: Metric, excludes: &[String], older_than: Option<u32>) -> String {
    let root = report.root();
    let scope = report.tree.root_path().to_string_lossy();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |time| time.as_secs());
    let measure = match metric {
        Metric::Allocated => "allocated",
        Metric::Apparent => "apparent",
    };
    let mut html = String::with_capacity(64 * 1024);
    let _ = write!(
        html,
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <title>{} · Space 16X</title><style>{STYLE}</style></head><body><main>\
         <h1>{}</h1><p>{} {measure} · {} apparent · {} items · scanned {}</p>",
        escape(&root.name().to_string_lossy()),
        escape(&scope),
        format_bytes(metric.bytes(root)),
        format_bytes(root.apparent_bytes()),
        thousands(root.item_count()),
        format_time(now),
    );
    if !report.issues.is_empty() {
        let _ = write!(
            html,
            "<p class=\"note\">Partial scan: {} {} couldn't be read, so the totals leave them out.</p>",
            thousands(report.issues.len() as u64),
            if report.issues.len() == 1 {
                "path"
            } else {
                "paths"
            }
        );
    }
    if let Some(days) = older_than {
        let _ = write!(
            html,
            "<p>Counts only files last modified more than {days} days ago.</p>"
        );
    }
    if !excludes.is_empty() {
        let patterns: Vec<String> = excludes.iter().map(|pattern| escape(pattern)).collect();
        let _ = write!(html, "<p>Excluded: {}</p>", patterns.join(", "));
    }

    let mut children: Vec<Entry<'_>> = root.children().collect();
    children.sort_by(|left, right| {
        metric
            .bytes(*right)
            .cmp(&metric.bytes(*left))
            .then_with(|| left.name().cmp(right.name()))
    });
    write_map(&mut html, &children, metric);
    write_contents(&mut html, &children, metric, metric.bytes(root));
    write_files(&mut html, report, metric);
    write_types(&mut html, root, metric);
    write_owners(&mut html, root, metric);
    let _ = writeln!(
        html,
        "<footer>Made with Space 16X {} (pro). Sizes use decimal units (1 KB = 1,000 bytes). \
         Review the paths on this page before you share it.</footer></main></body></html>",
        env!("CARGO_PKG_VERSION")
    );
    html
}

fn write_map(html: &mut String, children: &[Entry<'_>], metric: Metric) {
    let mut cells: Vec<(String, u64, &str)> = children
        .iter()
        .take(MAP_ITEMS)
        .filter(|child| metric.bytes(**child) > 0)
        .map(|child| {
            let kind = if child.kind() == EntryKind::Directory {
                "folder"
            } else {
                "file"
            };
            (
                child.name().to_string_lossy().into_owned(),
                metric.bytes(*child),
                kind,
            )
        })
        .collect();
    let rest = &children[children.len().min(MAP_ITEMS)..];
    let rest_bytes: u64 = rest.iter().map(|child| metric.bytes(*child)).sum();
    if rest_bytes > 0 {
        cells.push((format!("{} more items", rest.len()), rest_bytes, "other"));
        cells.sort_by_key(|cell| std::cmp::Reverse(cell.1));
    }
    if cells.is_empty() {
        return;
    }
    let sizes: Vec<u64> = cells.iter().map(|cell| cell.1).collect();
    let area = Rect {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 100.0,
    };
    html.push_str("<h2>Treemap</h2><div class=\"map\">");
    for ((name, bytes, kind), rect) in cells.iter().zip(treemap::layout(&sizes, area)) {
        let label = format!("{} · {}", escape(name), format_bytes(*bytes));
        let _ = write!(
            html,
            "<div class=\"cell {kind}\" style=\"left:{:.3}%;top:{:.3}%;width:{:.3}%;height:{:.3}%\" \
             title=\"{label}\"><b>{}</b>{}</div>",
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            escape(name),
            format_bytes(*bytes)
        );
    }
    html.push_str("</div>");
}

fn write_contents(html: &mut String, children: &[Entry<'_>], metric: Metric, total: u64) {
    html.push_str(
        "<h2>Contents</h2><table><tr><th>Name</th><th class=\"n\">Size</th>\
         <th class=\"n\">Items</th><th class=\"n\">Share</th><th style=\"width:18%\"></th></tr>",
    );
    for child in children.iter().take(TABLE_ROWS) {
        let bytes = metric.bytes(*child);
        let share = if total == 0 {
            0.0
        } else {
            bytes as f64 * 100.0 / total as f64
        };
        let directory = child.kind() == EntryKind::Directory;
        let _ = write!(
            html,
            "<tr><td class=\"p\">{}{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td>\
             <td class=\"n\">{share:.1}%</td><td><div class=\"bar\" style=\"width:{share:.1}%\"></div></td></tr>",
            escape(&child.name().to_string_lossy()),
            if directory { "/" } else { "" },
            format_bytes(bytes),
            if directory {
                thousands(child.item_count())
            } else {
                String::new()
            },
        );
    }
    more_rows(html, children.len(), TABLE_ROWS, 5);
    html.push_str("</table>");
}

fn write_files(html: &mut String, report: &Report, metric: Metric) {
    let files = largest_files(report.root(), TOP_FILES, metric);
    if files.is_empty() {
        return;
    }
    html.push_str("<h2>Largest files</h2><table><tr><th class=\"n\">Size</th><th>Path</th></tr>");
    for id in files {
        let entry = report.tree.entry(id);
        let _ = write!(
            html,
            "<tr><td class=\"n\">{}</td><td class=\"p\">{}</td></tr>",
            format_bytes(metric.bytes(entry)),
            escape(&entry.path().to_string_lossy())
        );
    }
    html.push_str("</table>");
}

fn write_types(html: &mut String, root: Entry<'_>, metric: Metric) {
    let types = super::type_breakdown(root, metric);
    if types.is_empty() {
        return;
    }
    html.push_str(
        "<h2>File types</h2><table><tr><th>Type</th><th class=\"n\">Size</th>\
         <th class=\"n\">Files</th></tr>",
    );
    for kind in types.iter().take(TYPES) {
        let bytes = match metric {
            Metric::Allocated => kind.allocated_bytes,
            Metric::Apparent => kind.apparent_bytes,
        };
        let _ = write!(
            html,
            "<tr><td>{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td></tr>",
            escape(&kind.label()),
            format_bytes(bytes),
            thousands(kind.file_count)
        );
    }
    more_rows(html, types.len(), TYPES, 3);
    html.push_str("</table>");
}

/// Owners, when more than one user owns items, as on a shared server.
fn write_owners(html: &mut String, root: Entry<'_>, metric: Metric) {
    let list = owners::breakdown(root, metric, owners::user_name);
    if list.len() < 2 {
        return;
    }
    html.push_str(
        "<h2>Owners</h2><table><tr><th>Owner</th><th class=\"n\">Size</th>\
         <th class=\"n\">Items</th></tr>",
    );
    for owner in list.iter().take(TABLE_ROWS) {
        let _ = write!(
            html,
            "<tr><td>{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td></tr>",
            escape(&owner.label()),
            format_bytes(owner.bytes(metric)),
            thousands(owner.item_count())
        );
    }
    html.push_str("</table>");
}

/// A last row saying how many rows past `limit` aren't listed.
fn more_rows(html: &mut String, count: usize, limit: usize, columns: usize) {
    if count > limit {
        let _ = write!(
            html,
            "<tr><td colspan=\"{columns}\">and {} more</td></tr>",
            thousands((count - limit) as u64)
        );
    }
}

/// Escapes text for HTML content and attribute values.
fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

fn thousands(value: u64) -> String {
    let digits = value.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::{EntryState, TreeBuilder};
    use std::sync::Arc;

    #[test]
    fn writes_a_self_contained_page() {
        let mut builder = TreeBuilder::new("/srv/<shared>".into(), EntryState::Local);
        builder.open_folder(b"videos & clips", EntryState::Local);
        builder.file(b"talk.mov", 9_000_000, 9_000_000, EntryState::Local);
        builder.close_folder();
        builder.file(b"notes \"draft\".txt", 4_000, 3_000, EntryState::Local);
        let report = Report {
            tree: Arc::new(builder.finish().0),
            issues: Vec::new(),
            excluded_count: 0,
            is_preview: false,
        };
        let html = page(&report, Metric::Allocated, &["*.tmp".into()], Some(30));
        assert!(html.starts_with("<!doctype html>"));
        assert!(html.trim_end().ends_with("</html>"));
        // Nothing runs and nothing loads from elsewhere.
        for forbidden in ["<script", "http://", "https://", "<link", "<img", "url("] {
            assert!(!html.contains(forbidden), "{forbidden}");
        }
        assert!(html.contains("<h1>/srv/&lt;shared&gt;</h1>"));
        assert!(html.contains("videos &amp; clips/"));
        assert!(html.contains("notes &quot;draft&quot;.txt"));
        assert!(html.contains("class=\"cell folder\""));
        assert!(html.contains("Counts only files last modified more than 30 days ago."));
        assert!(html.contains("Excluded: *.tmp"));
        assert!(html.contains("<h2>Largest files</h2>"));
        assert!(html.contains("<td>.mov</td>"));
    }

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Structure {
        heading: String,
        sections: Vec<String>,
        contents: Vec<String>,
        largest_files: Vec<String>,
        file_types: Vec<String>,
        forbidden: Vec<String>,
    }

    /// The text of each cell of `class` in the section headed `heading`.
    fn cells(html: &str, heading: &str, class: &str) -> Vec<String> {
        let start = html.find(&format!("<h2>{heading}</h2>")).expect(heading);
        let section = &html[start + 4..];
        let section = &section[..section.find("<h2>").unwrap_or(section.len())];
        let marker = format!("<td{class}>");
        section
            .split(&marker)
            .skip(1)
            .map(|cell| cell[..cell.find("</td>").unwrap()].to_owned())
            .collect()
    }

    /// `HTMLReportTests` in the Mac app checks its page against the same structure.
    #[test]
    fn matches_the_shared_structure() {
        let expected: Structure =
            serde_json::from_str(include_str!("../../../fixtures/html-report/expected.json"))
                .unwrap();
        let mut input: &[u8] = include_bytes!("../../../fixtures/cleanup/scan.json");
        let report = crate::ncdu::read(&mut |buffer| io::Read::read(&mut input, buffer))
            .unwrap()
            .report;
        let html = page(&report, Metric::Allocated, &[], None);
        assert!(html.contains(&format!("<h1>{}</h1>", expected.heading)));
        let sections: Vec<&str> = html
            .split("<h2>")
            .skip(1)
            .map(|part| &part[..part.find("</h2>").unwrap()])
            .collect();
        assert_eq!(sections, expected.sections);
        let contents = cells(&html, "Contents", " class=\"p\"");
        assert_eq!(&contents[..expected.contents.len()], expected.contents);
        let files = cells(&html, "Largest files", " class=\"p\"");
        assert_eq!(
            &files[..expected.largest_files.len()],
            expected.largest_files
        );
        // Each app names files without an extension its own way.
        let types: Vec<String> = cells(&html, "File types", "")
            .into_iter()
            .filter(|label| label.starts_with('.'))
            .collect();
        assert_eq!(&types[..expected.file_types.len()], expected.file_types);
        for forbidden in &expected.forbidden {
            assert!(!html.contains(forbidden.as_str()), "{forbidden}");
        }
    }

    #[test]
    fn groups_digits() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_000), "1,000");
        assert_eq!(thousands(1_234_567), "1,234,567");
        assert_eq!(escape("a<'b'>&\""), "a&lt;&#39;b&#39;&gt;&amp;&quot;");
    }
}
