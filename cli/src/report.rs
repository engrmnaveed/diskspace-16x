use std::borrow::Cow;
use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;
use std::fmt::Write as _;
use std::io::{self, Write};
use std::path::Path;

use serde::Serialize;

use crate::scanner::{Entry, EntryKind, EntryState, NodeId, Report, Tree};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Selection {
    Depth(usize),
    Top(usize),
}

/// The size that reports show and rank by.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Metric {
    /// Disk space the files occupy (`st_blocks`), the default.
    #[default]
    Allocated,
    /// File lengths (`st_size`), which differ for sparse, compressed, and small files.
    Apparent,
}

impl Metric {
    pub(crate) fn bytes(self, entry: Entry<'_>) -> u64 {
        match self {
            Self::Allocated => entry.allocated_bytes(),
            Self::Apparent => entry.apparent_bytes(),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Allocated => "allocated",
            Self::Apparent => "apparent",
        }
    }
}

pub(crate) fn write_plain(
    out: impl Write,
    report: &Report,
    selection: Selection,
    metric: Metric,
) -> io::Result<()> {
    let mut out = io::BufWriter::new(out);
    let tree = &report.tree;
    let entries = match selection {
        Selection::Depth(depth) => directory_entries(tree, depth, metric),
        Selection::Top(limit) => largest_files(tree.root(), limit, metric),
    };
    for id in entries {
        write_entry(&mut out, tree.entry(id), metric, "")?;
    }
    let total = if report.issues.is_empty() {
        "  [total]"
    } else {
        "  [partial total]"
    };
    write_entry(&mut out, report.root(), metric, total)?;
    out.flush()
}

fn write_entry(
    mut out: impl Write,
    entry: Entry<'_>,
    metric: Metric,
    annotation: &str,
) -> io::Result<()> {
    let state = match entry.state() {
        EntryState::CloudOnly => "  [cloud only]",
        EntryState::Protected => "  [protected]",
        EntryState::Incomplete => "  [incomplete]",
        EntryState::HardLinkAlias => "  [hard link counted elsewhere]",
        _ => "",
    };
    writeln!(
        out,
        "{:>10}  {}{state}{annotation}",
        format_bytes(metric.bytes(entry)),
        terminal_path(&entry.path()),
    )
}

/// Formats a size in decimal units, as Finder and the Mac app's `ByteCountFormatter` do.
pub(crate) fn format_bytes(bytes: u64) -> String {
    scaled(bytes, 1000, ["KB", "MB", "GB", "TB", "PB", "EB"])
}

/// Formats a size in binary units for detail views beside the decimal size.
pub(crate) fn format_binary_bytes(bytes: u64) -> String {
    scaled(bytes, 1024, ["KiB", "MiB", "GiB", "TiB", "PiB", "EiB"])
}

fn scaled(bytes: u64, base: u128, units: [&str; 6]) -> String {
    if u128::from(bytes) < base {
        return format!("{bytes} B");
    }
    // Kilo units show whole numbers, mega units one decimal, and larger units two.
    const DIGITS: [u32; 6] = [0, 1, 2, 2, 2, 2];
    let mut divisor = base;
    for (index, (unit, digits)) in units.into_iter().zip(DIGITS).enumerate() {
        let scale = 10_u128.pow(digits);
        // Integer arithmetic rounds ties up, matching ByteCountFormatter.
        let rounded = (u128::from(bytes) * scale * 2 + divisor) / (divisor * 2);
        if rounded < base * scale || index == units.len() - 1 {
            let (whole, fraction) = (rounded / scale, rounded % scale);
            return if digits == 0 {
                format!("{whole} {unit}")
            } else {
                format!("{whole}.{fraction:0width$} {unit}", width = digits as usize)
            };
        }
        divisor *= base;
    }
    unreachable!("the largest unit accepts every u64 byte count")
}

pub(crate) fn terminal_path(path: &Path) -> String {
    let path = path.to_string_lossy();
    if !path.chars().any(|c| c == '\\' || needs_escape(c)) {
        return path.into_owned();
    }
    let mut escaped = String::with_capacity(path.len());
    for character in path.chars() {
        if character == '\\' || needs_escape(character) {
            escaped.extend(character.escape_default());
        } else {
            escaped.push(character);
        }
    }
    escaped
}

pub(crate) fn terminal_text(text: &str) -> Cow<'_, str> {
    if !text.chars().any(needs_escape) {
        return Cow::Borrowed(text);
    }
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        if needs_escape(character) {
            escaped.extend(character.escape_default());
        } else {
            escaped.push(character);
        }
    }
    Cow::Owned(escaped)
}

/// Control characters can drive the terminal. Bidirectional controls can make a crafted
/// name display as different text, as in "Trojan Source" spoofing. Escape both kinds.
fn needs_escape(character: char) -> bool {
    character.is_control()
        || matches!(
            character,
            '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'
        )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonReport<'a> {
    schema_version: u8,
    edition: &'static str,
    scope: Cow<'a, str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scope_bytes: Option<String>,
    ranked_by: &'static str,
    allocated_bytes: u64,
    apparent_bytes: u64,
    item_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    newest_modified: Option<u32>,
    excluded_count: u64,
    excludes: &'a [String],
    /// Pro's `--older-than`: only files modified more than this many days ago count.
    #[serde(skip_serializing_if = "Option::is_none")]
    older_than_days: Option<u32>,
    is_partial: bool,
    issues: Vec<JsonIssue<'a>>,
    #[serde(flatten)]
    result: JsonSelection,
}

#[derive(Serialize)]
#[serde(untagged, rename_all_fields = "camelCase")]
enum JsonSelection {
    Depth {
        depth: usize,
        entries: Vec<JsonEntry>,
    },
    Top {
        top_limit: usize,
        top_files: Vec<JsonFile>,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonEntry {
    #[serde(flatten)]
    file: JsonFile,
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    item_count: Option<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonFile {
    #[serde(flatten)]
    path: JsonPath<'static>,
    allocated_bytes: u64,
    apparent_bytes: u64,
    /// Pro: the modification time, or a folder's newest one below it, in Unix seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    newest_modified: Option<u32>,
}

impl From<Entry<'_>> for JsonFile {
    fn from(entry: Entry<'_>) -> Self {
        Self {
            path: JsonPath::owned(&entry.path()),
            allocated_bytes: entry.allocated_bytes(),
            apparent_bytes: entry.apparent_bytes(),
            newest_modified: entry.details().modified(),
        }
    }
}

#[derive(Serialize)]
struct JsonIssue<'a> {
    #[serde(flatten)]
    path: JsonPath<'a>,
    message: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JsonPath<'a> {
    pub(crate) path: Cow<'a, str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) path_bytes: Option<String>,
}

impl JsonPath<'static> {
    fn owned(path: &Path) -> Self {
        let JsonPath { path, path_bytes } = JsonPath::from(path);
        JsonPath {
            path: Cow::Owned(path.into_owned()),
            path_bytes,
        }
    }
}

impl<'a> From<&'a Path> for JsonPath<'a> {
    fn from(path: &'a Path) -> Self {
        let display = path.to_string_lossy();
        let path_bytes = if matches!(display, Cow::Owned(_)) {
            let bytes = path.as_os_str().as_encoded_bytes();
            let mut hex = String::with_capacity(bytes.len() * 2);
            for byte in bytes {
                write!(hex, "{byte:02x}").expect("String writes are infallible");
            }
            Some(hex)
        } else {
            None
        };
        Self {
            path: display,
            path_bytes,
        }
    }
}

pub(crate) fn write_json(
    out: impl Write,
    report: &Report,
    selection: Selection,
    metric: Metric,
    excludes: &[String],
    older_than_days: Option<u32>,
) -> io::Result<()> {
    let mut out = io::BufWriter::new(out);
    let tree = &report.tree;
    let scope = JsonPath::from(tree.root_path());
    let result = match selection {
        Selection::Depth(depth) => JsonSelection::Depth {
            depth,
            entries: directory_entries(tree, depth, metric)
                .into_iter()
                .map(|id| {
                    let entry = tree.entry(id);
                    let directory = entry.kind() == EntryKind::Directory;
                    JsonEntry {
                        file: JsonFile::from(entry),
                        kind: if directory { "directory" } else { "file" },
                        item_count: directory.then_some(entry.item_count()),
                    }
                })
                .collect(),
        },
        Selection::Top(top_limit) => JsonSelection::Top {
            top_limit,
            top_files: largest_files(tree.root(), top_limit, metric)
                .into_iter()
                .map(|id| JsonFile::from(tree.entry(id)))
                .collect(),
        },
    };
    let json = JsonReport {
        schema_version: 3,
        edition: EDITION,
        scope: scope.path,
        scope_bytes: scope.path_bytes,
        ranked_by: metric.name(),
        allocated_bytes: report.root().allocated_bytes(),
        apparent_bytes: report.root().apparent_bytes(),
        item_count: report.root().item_count(),
        newest_modified: report.root().details().modified(),
        excluded_count: report.excluded_count,
        excludes,
        older_than_days,
        is_partial: !report.issues.is_empty(),
        issues: report
            .issues
            .iter()
            .map(|issue| JsonIssue {
                path: JsonPath::from(issue.path.as_path()),
                message: &issue.message,
            })
            .collect(),
        result,
    };
    serde_json::to_writer_pretty(&mut out, &json)?;
    writeln!(out)?;
    out.flush()
}

/// The build's edition, which reports and `--version` show.
pub(crate) const EDITION: &str = if cfg!(feature = "pro") {
    "pro"
} else {
    "basic"
};

/// Entries through `depth` levels below the root, each folder's contents ranked.
fn directory_entries(tree: &Tree, depth: usize, metric: Metric) -> Vec<NodeId> {
    let mut output = Vec::new();
    let mut pending = Vec::new();
    let mut entry = tree.root();
    let mut remaining = depth;
    loop {
        if remaining > 0 {
            let mut children: Vec<_> = entry.children().collect();
            // Sort in reverse output order so the next child is on top of the stack.
            children.sort_unstable_by(|&left, &right| rank(left, right, metric));
            pending.extend(children.into_iter().map(|child| (child, remaining - 1)));
        }
        let Some((next, next_depth)) = pending.pop() else {
            break;
        };
        output.push(next.id());
        entry = next;
        remaining = next_depth;
    }
    output
}

/// The `limit` largest counted files below `scope`, largest first.
pub(crate) fn largest_files(scope: Entry<'_>, limit: usize, metric: Metric) -> Vec<NodeId> {
    if limit == 0 {
        return Vec::new();
    }
    let mut heap = BinaryHeap::new();
    for entry in scope.descendants() {
        if entry.kind() != EntryKind::File || entry.state() != EntryState::Local {
            continue;
        }
        let candidate = Reverse(RankedFile(entry, metric));
        if heap.len() < limit {
            heap.push(candidate);
        } else if let Some(mut smallest) = heap.peek_mut()
            && candidate < *smallest
        {
            *smallest = candidate;
        }
    }
    heap.into_sorted_vec()
        .into_iter()
        .map(|Reverse(file)| file.0.id())
        .collect()
}

/// Orders by size, then by path with the smaller path ranking higher.
fn rank(left: Entry<'_>, right: Entry<'_>, metric: Metric) -> Ordering {
    metric
        .bytes(left)
        .cmp(&metric.bytes(right))
        .then_with(|| left.tree().cmp_paths(right.id(), left.id()))
}

#[derive(Clone, Copy)]
struct RankedFile<'a>(Entry<'a>, Metric);

impl PartialEq for RankedFile<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for RankedFile<'_> {}

impl PartialOrd for RankedFile<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RankedFile<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        rank(self.0, other.0, self.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_terminal_controls_and_bidirectional_overrides() {
        assert_eq!(
            terminal_path(Path::new("/data/invoice\u{202E}fdp.exe")),
            "/data/invoice\\u{202e}fdp.exe"
        );
        assert_eq!(
            terminal_text("Listing\u{1b}[31m \u{2067}name\u{2069}"),
            "Listing\\u{1b}[31m \\u{2067}name\\u{2069}"
        );
        assert_eq!(terminal_path(Path::new("/données/👨‍👩‍👧")), "/données/👨‍👩‍👧");
        assert_eq!(terminal_path(Path::new("/a\\b")), "/a\\\\b");
    }

    #[test]
    fn formats_decimal_sizes_like_byte_count_formatter() {
        // Expected strings are ByteCountFormatter's .file output on macOS.
        for (bytes, expected) in [
            (0, "0 B"),
            (999, "999 B"),
            (1_000, "1 KB"),
            (1_499, "1 KB"),
            (1_500, "2 KB"),
            (2_500, "3 KB"),
            (999_499, "999 KB"),
            (999_500, "1.0 MB"),
            (1_250_000, "1.3 MB"),
            (999_949_999, "999.9 MB"),
            (999_950_000, "1.00 GB"),
            (2_005_000_000, "2.01 GB"),
            (45_888_520_192, "45.89 GB"),
            (999_994_999_999, "999.99 GB"),
            (999_995_000_000, "1.00 TB"),
            (1_234_567_890_123, "1.23 TB"),
            (9_223_372_036_854_775_807, "9.22 EB"),
            (u64::MAX, "18.45 EB"),
        ] {
            assert_eq!(format_bytes(bytes), expected, "{bytes} bytes");
        }
    }

    #[test]
    fn formats_binary_sizes_with_iec_units() {
        // Keep in sync with ByteFormatTests in the Swift package.
        for (bytes, expected) in [
            (1_023, "1023 B"),
            (1_024, "1 KiB"),
            (1_535, "1 KiB"),
            (1_536, "2 KiB"),
            (2_560, "3 KiB"),
            (1_048_063, "1023 KiB"),
            (1_048_064, "1.0 MiB"),
            (1_101_004, "1.0 MiB"),
            (1_101_005, "1.1 MiB"),
            (1_073_689_395, "1023.9 MiB"),
            (1_073_689_396, "1.00 GiB"),
            (45_888_520_192, "42.74 GiB"),
            (245_349_000_000, "228.50 GiB"),
            (1_099_506_259_066, "1023.99 GiB"),
            (1_099_506_259_067, "1.00 TiB"),
            (u64::MAX, "16.00 EiB"),
        ] {
            assert_eq!(format_binary_bytes(bytes), expected, "{bytes} bytes");
        }
    }
}
