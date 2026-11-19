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

pub(crate) fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    for unit in ["KiB", "MiB", "GiB", "TiB", "PiB", "EiB"] {
        value /= 1024.0;
        if value < 1024.0 || unit == "EiB" {
            return format!("{value:.1} {unit}");
        }
    }
    unreachable!("there is a unit for every u64 byte count")
}

pub(crate) fn terminal_path(path: &Path) -> String {
    let path = path.to_string_lossy();
    if !path.chars().any(|c| c == '\\' || c.is_control()) {
        return path.into_owned();
    }
    let mut escaped = String::with_capacity(path.len());
    for character in path.chars() {
        if character == '\\' || character.is_control() {
            escaped.extend(character.escape_default());
        } else {
            escaped.push(character);
        }
    }
    escaped
}

pub(crate) fn terminal_text(text: &str) -> Cow<'_, str> {
    if !text.chars().any(char::is_control) {
        return Cow::Borrowed(text);
    }
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        if character.is_control() {
            escaped.extend(character.escape_default());
        } else {
            escaped.push(character);
        }
    }
    Cow::Owned(escaped)
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
