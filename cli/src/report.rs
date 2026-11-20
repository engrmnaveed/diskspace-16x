use std::borrow::Cow;
use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;
use std::fmt::Write as _;
use std::io::{self, Write};
use std::path::Path;
use std::sync::Arc;

use serde::Serialize;

use crate::scanner::{Entry, EntryKind, EntryState, Report};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Selection {
    Depth(usize),
    Top(usize),
}

pub(crate) fn write_plain(
    out: impl Write,
    report: &Report,
    selection: Selection,
) -> io::Result<()> {
    let mut out = io::BufWriter::new(out);
    match selection {
        Selection::Depth(depth) => {
            for entry in directory_entries(&report.root, depth) {
                write_entry(&mut out, entry, "")?;
            }
        }
        Selection::Top(limit) => {
            for entry in largest_files(&report.root, limit) {
                write_entry(&mut out, entry, "")?;
            }
        }
    }
    let total = if report.issues.is_empty() {
        "  [total]"
    } else {
        "  [partial total]"
    };
    write_entry(&mut out, &report.root, total)?;
    out.flush()
}

fn write_entry(mut out: impl Write, entry: &Entry, annotation: &str) -> io::Result<()> {
    let state = match entry.state {
        EntryState::CloudOnly => "  [cloud only]",
        EntryState::Protected => "  [protected]",
        EntryState::Incomplete => "  [incomplete]",
        EntryState::HardLinkAlias => "  [hard link counted elsewhere]",
        _ => "",
    };
    writeln!(
        out,
        "{:>10}  {}{state}{annotation}",
        format_bytes(entry.allocated_bytes),
        terminal_path(&entry.path),
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
    scope: Cow<'a, str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scope_bytes: Option<String>,
    allocated_bytes: u64,
    is_partial: bool,
    issues: Vec<JsonIssue<'a>>,
    #[serde(flatten)]
    result: JsonSelection<'a>,
}

#[derive(Serialize)]
#[serde(untagged, rename_all_fields = "camelCase")]
enum JsonSelection<'a> {
    Depth {
        depth: usize,
        entries: Vec<JsonEntry<'a>>,
    },
    Top {
        top_limit: usize,
        top_files: Vec<JsonFile<'a>>,
    },
}

#[derive(Serialize)]
struct JsonEntry<'a> {
    #[serde(flatten)]
    file: JsonFile<'a>,
    kind: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonFile<'a> {
    #[serde(flatten)]
    path: JsonPath<'a>,
    allocated_bytes: u64,
}

impl<'a> From<&'a Entry> for JsonFile<'a> {
    fn from(entry: &'a Entry) -> Self {
        Self {
            path: JsonPath::from(entry.path.as_path()),
            allocated_bytes: entry.allocated_bytes,
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
struct JsonPath<'a> {
    path: Cow<'a, str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path_bytes: Option<String>,
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

pub(crate) fn write_json(out: impl Write, report: &Report, selection: Selection) -> io::Result<()> {
    let mut out = io::BufWriter::new(out);
    let scope = JsonPath::from(report.root.path.as_path());
    let result = match selection {
        Selection::Depth(depth) => JsonSelection::Depth {
            depth,
            entries: directory_entries(&report.root, depth)
                .into_iter()
                .map(|entry| JsonEntry {
                    file: JsonFile::from(entry),
                    kind: if entry.kind == EntryKind::Directory {
                        "directory"
                    } else {
                        "file"
                    },
                })
                .collect(),
        },
        Selection::Top(top_limit) => JsonSelection::Top {
            top_limit,
            top_files: largest_files(&report.root, top_limit)
                .into_iter()
                .map(|entry| JsonFile::from(entry.as_ref()))
                .collect(),
        },
    };
    let json = JsonReport {
        schema_version: 2,
        scope: scope.path,
        scope_bytes: scope.path_bytes,
        allocated_bytes: report.root.allocated_bytes,
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

fn directory_entries(root: &Entry, depth: usize) -> Vec<&Entry> {
    let mut output = Vec::new();
    let mut pending = Vec::new();
    let mut entry = root;
    let mut remaining = depth;
    loop {
        if remaining > 0 && !entry.children.is_empty() {
            let mut children: Vec<_> = entry.children.iter().map(AsRef::as_ref).collect();
            // Sort in reverse output order so the next child is on top of the stack.
            children.sort_unstable_by(|left, right| rank(left, right));
            pending.extend(children.into_iter().map(|child| (child, remaining - 1)));
        }
        let Some((next, next_depth)) = pending.pop() else {
            break;
        };
        output.push(next);
        entry = next;
        remaining = next_depth;
    }
    output
}

pub(crate) fn largest_files(root: &Entry, limit: usize) -> Vec<&Arc<Entry>> {
    if limit == 0 {
        return Vec::new();
    }
    let mut heap = BinaryHeap::new();
    let mut levels = vec![root.children.iter()];
    while let Some(level) = levels.last_mut() {
        let Some(entry) = level.next() else {
            levels.pop();
            continue;
        };
        if entry.kind == EntryKind::Directory {
            levels.push(entry.children.iter());
        } else if entry.state == EntryState::Local {
            let candidate = Reverse(RankedFile(entry));
            if heap.len() < limit {
                heap.push(candidate);
            } else if let Some(mut smallest) = heap.peek_mut()
                && candidate < *smallest
            {
                *smallest = candidate;
            }
        }
    }
    heap.into_sorted_vec()
        .into_iter()
        .map(|Reverse(file)| file.0)
        .collect()
}

fn rank(left: &Entry, right: &Entry) -> Ordering {
    left.allocated_bytes
        .cmp(&right.allocated_bytes)
        .then_with(|| {
            right
                .path
                .as_os_str()
                .as_encoded_bytes()
                .cmp(left.path.as_os_str().as_encoded_bytes())
        })
}

#[derive(Clone, Copy)]
struct RankedFile<'a>(&'a Arc<Entry>);

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
        rank(self.0, other.0)
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
