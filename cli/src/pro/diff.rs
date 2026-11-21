//! Compares two scans of the same folder, matching entries by their path below each root.

use std::cmp::Reverse;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::io::{self, Write};
use std::path::PathBuf;

use serde::Serialize;

use crate::report::{self, JsonPath, Metric, Selection};
use crate::scanner::{Entry, EntryKind, EntryState, Report, Tree};

/// One file or folder whose size differs between the scans.
#[derive(Debug, PartialEq, Eq)]
pub struct Change {
    /// The path in the new scan, or where the entry was for one only the old scan has.
    pub path: PathBuf,
    pub kind: EntryKind,
    /// None when the entry is only in the other scan.
    pub old: Option<u64>,
    pub new: Option<u64>,
}

impl Change {
    pub fn delta(&self) -> i128 {
        i128::from(self.new.unwrap_or(0)) - i128::from(self.old.unwrap_or(0))
    }
}

/// An entry's counterpart in the other scan: the same name and kind in the matching folder.
fn counterparts<'a>(folder: Option<Entry<'a>>) -> HashMap<&'a OsStr, Entry<'a>> {
    folder
        .into_iter()
        .flat_map(Entry::children)
        .map(|child| (child.name(), child))
        .collect()
}

/// Bytes an entry counts. A hard-link alias counts nothing, as in every report.
fn bytes(entry: Entry<'_>, metric: Metric) -> u64 {
    if entry.state() == EntryState::HardLinkAlias {
        0
    } else {
        metric.bytes(entry)
    }
}

/// A folder in both scans, or in one of them, with its path in the new scan's terms.
struct Pair<'a> {
    path: PathBuf,
    old: Option<Entry<'a>>,
    new: Option<Entry<'a>>,
}

/// The children of `pair` that changed, and the folder pairs below it.
fn compare_children<'a>(pair: &Pair<'a>, metric: Metric) -> (Vec<Change>, Vec<Pair<'a>>) {
    let mut old = counterparts(pair.old);
    let mut changes = Vec::new();
    let mut folders = Vec::new();
    let mut add = |path: PathBuf, old: Option<Entry<'a>>, new: Option<Entry<'a>>| {
        let entry = new.or(old).expect("one side has the entry");
        let change = Change {
            path: path.clone(),
            kind: entry.kind(),
            old: old.map(|entry| bytes(entry, metric)),
            new: new.map(|entry| bytes(entry, metric)),
        };
        if change.delta() != 0 || old.is_none() != new.is_none() {
            changes.push(change);
        }
        if entry.kind() == EntryKind::Directory {
            folders.push(Pair { path, old, new });
        }
    };
    for child in pair.new.into_iter().flat_map(Entry::children) {
        let path = pair.path.join(child.name());
        match old.remove(child.name()) {
            Some(before) if before.kind() == child.kind() => add(path, Some(before), Some(child)),
            Some(before) => {
                add(path.clone(), Some(before), None);
                add(path, None, Some(child));
            }
            None => add(path, None, Some(child)),
        }
    }
    let mut removed: Vec<_> = old.into_values().collect();
    removed.sort_by(|left, right| left.name().cmp(right.name()));
    for before in removed {
        add(pair.path.join(before.name()), Some(before), None);
    }
    (changes, folders)
}

/// Ranks larger changes first, either way, with ties in path order.
fn rank(changes: &mut [Change]) {
    changes.sort_by(|left, right| {
        Reverse(left.delta().unsigned_abs())
            .cmp(&Reverse(right.delta().unsigned_abs()))
            .then_with(|| left.path.as_os_str().cmp(right.path.as_os_str()))
    });
}

fn roots<'a>(old: &'a Tree, new: &'a Tree) -> Pair<'a> {
    Pair {
        path: new.root_path().to_path_buf(),
        old: Some(old.root()),
        new: Some(new.root()),
    }
}

/// Changed entries through `depth` levels below the roots, in tree order, with each
/// folder's changes ranked largest first. Entries only one scan has are included, and
/// folders whose total didn't change aren't expanded.
pub fn entries(old: &Tree, new: &Tree, depth: usize, metric: Metric) -> Vec<Change> {
    let mut output = Vec::new();
    if depth == 0 {
        return output;
    }
    // Each open level's changes in reverse, so the next one is on top, with the depth
    // left below that level.
    let mut stack = vec![(level(&roots(old, new), metric), depth)];
    while let Some((changes, remaining)) = stack.last_mut() {
        let remaining = *remaining;
        let Some((change, folder)) = changes.pop() else {
            stack.pop();
            continue;
        };
        output.push(change);
        if let Some(folder) = folder
            && remaining > 1
        {
            stack.push((level(&folder, metric), remaining - 1));
        }
    }
    output
}

/// A folder's changes ranked in reverse, each changed folder with its pair to expand.
fn level<'a>(pair: &Pair<'a>, metric: Metric) -> Vec<(Change, Option<Pair<'a>>)> {
    let (mut changes, folders) = compare_children(pair, metric);
    rank(&mut changes);
    let mut folders: Vec<Option<Pair<'a>>> = folders.into_iter().map(Some).collect();
    let mut level: Vec<_> = changes
        .into_iter()
        .map(|change| {
            let folder = (change.kind == EntryKind::Directory)
                .then(|| {
                    folders.iter_mut().find_map(|slot| {
                        let matches = slot.as_ref().is_some_and(|folder| {
                            folder.path == change.path
                                && folder.old.is_some() == change.old.is_some()
                                && folder.new.is_some() == change.new.is_some()
                        });
                        if matches { slot.take() } else { None }
                    })
                })
                .flatten();
            (change, folder)
        })
        .collect();
    level.reverse();
    level
}

/// The `limit` files whose size changed the most, including files only one scan has.
pub fn files(old: &Tree, new: &Tree, limit: usize, metric: Metric) -> Vec<Change> {
    if limit == 0 {
        return Vec::new();
    }
    let mut changed = Vec::new();
    let mut pending = vec![roots(old, new)];
    while let Some(pair) = pending.pop() {
        let (changes, folders) = compare_children(&pair, metric);
        changed.extend(
            changes
                .into_iter()
                .filter(|change| change.kind == EntryKind::File && change.delta() != 0),
        );
        pending.extend(folders);
    }
    rank(&mut changed);
    changed.truncate(limit);
    changed
}

fn signed(delta: i128) -> String {
    let size = report::format_bytes(u64::try_from(delta.unsigned_abs()).unwrap_or(u64::MAX));
    match delta {
        0 => size,
        ..0 => format!("-{size}"),
        _ => format!("+{size}"),
    }
}

/// Totals of both scans and how they changed.
pub struct Summary<'a> {
    pub old: &'a Report,
    pub new: &'a Report,
    pub metric: Metric,
}

impl Summary<'_> {
    fn total(&self) -> Change {
        Change {
            path: self.new.tree.root_path().to_path_buf(),
            kind: EntryKind::Directory,
            old: Some(self.metric.bytes(self.old.root())),
            new: Some(self.metric.bytes(self.new.root())),
        }
    }
}

pub fn write_plain(out: impl Write, summary: &Summary<'_>, changes: &[Change]) -> io::Result<()> {
    let mut out = io::BufWriter::new(out);
    writeln!(out, "{:>11} {:>10} {:>10}  Path", "Change", "Old", "New")?;
    let line = |out: &mut io::BufWriter<_>, change: &Change, note: &str| {
        let size = |bytes: Option<u64>| bytes.map_or_else(|| "-".into(), report::format_bytes);
        let state = match (change.old, change.new) {
            (None, Some(_)) => "  [added]",
            (Some(_), None) => "  [removed]",
            _ => "",
        };
        writeln!(
            out,
            "{:>11} {:>10} {:>10}  {}{state}{note}",
            signed(change.delta()),
            size(change.old),
            size(change.new),
            report::terminal_path(&change.path),
        )
    };
    for change in changes {
        line(&mut out, change, "")?;
    }
    let partial = !summary.old.issues.is_empty() || !summary.new.issues.is_empty();
    let note = if partial {
        "  [partial total]"
    } else {
        "  [total]"
    };
    line(&mut out, &summary.total(), note)?;
    out.flush()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonDiff<'a> {
    schema_version: u8,
    edition: &'static str,
    ranked_by: &'static str,
    old: JsonScan<'a>,
    new: JsonScan<'a>,
    change_bytes: i128,
    #[serde(flatten)]
    result: JsonResult<'a>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonScan<'a> {
    scope: std::borrow::Cow<'a, str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scope_bytes: Option<String>,
    allocated_bytes: u64,
    apparent_bytes: u64,
    item_count: u64,
    is_partial: bool,
}

impl<'a> From<&'a Report> for JsonScan<'a> {
    fn from(report: &'a Report) -> Self {
        let scope = JsonPath::from(report.tree.root_path());
        Self {
            scope: scope.path,
            scope_bytes: scope.path_bytes,
            allocated_bytes: report.root().allocated_bytes(),
            apparent_bytes: report.root().apparent_bytes(),
            item_count: report.root().item_count(),
            is_partial: !report.issues.is_empty(),
        }
    }
}

#[derive(Serialize)]
#[serde(untagged, rename_all_fields = "camelCase")]
enum JsonResult<'a> {
    Depth {
        depth: usize,
        entries: Vec<JsonChange<'a>>,
    },
    Top {
        top_limit: usize,
        top_files: Vec<JsonChange<'a>>,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonChange<'a> {
    #[serde(flatten)]
    path: JsonPath<'a>,
    kind: &'static str,
    old_bytes: Option<u64>,
    new_bytes: Option<u64>,
    change_bytes: i128,
}

impl<'a> From<&'a Change> for JsonChange<'a> {
    fn from(change: &'a Change) -> Self {
        Self {
            path: JsonPath::from(change.path.as_path()),
            kind: match change.kind {
                EntryKind::Directory => "directory",
                EntryKind::File => "file",
            },
            old_bytes: change.old,
            new_bytes: change.new,
            change_bytes: change.delta(),
        }
    }
}

pub fn write_json(
    out: impl Write,
    summary: &Summary<'_>,
    selection: Selection,
    changes: &[Change],
) -> io::Result<()> {
    let mut out = io::BufWriter::new(out);
    let list = changes.iter().map(JsonChange::from).collect();
    let json = JsonDiff {
        schema_version: 3,
        edition: report::EDITION,
        ranked_by: match summary.metric {
            Metric::Allocated => "allocated",
            Metric::Apparent => "apparent",
        },
        old: JsonScan::from(summary.old),
        new: JsonScan::from(summary.new),
        change_bytes: summary.total().delta(),
        result: match selection {
            Selection::Depth(depth) => JsonResult::Depth {
                depth,
                entries: list,
            },
            Selection::Top(top_limit) => JsonResult::Top {
                top_limit,
                top_files: list,
            },
        },
    };
    serde_json::to_writer_pretty(&mut out, &json)?;
    writeln!(out)?;
    out.flush()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::scanner::TreeBuilder;

    fn relative(change: &Change, root: &Path) -> String {
        change
            .path
            .strip_prefix(root)
            .unwrap_or(&change.path)
            .to_string_lossy()
            .into_owned()
    }

    /// /r with a (x 100, y 50), b (z 10), keep 5, gone 7, and swap as a file.
    fn before() -> Tree {
        let mut builder = TreeBuilder::new("/old/r".into(), EntryState::Local);
        builder.open_folder(b"a", EntryState::Local);
        builder.file(b"x", 100, 90, EntryState::Local);
        builder.file(b"y", 50, 50, EntryState::Local);
        builder.close_folder();
        builder.open_folder(b"b", EntryState::Local);
        builder.file(b"z", 10, 10, EntryState::Local);
        builder.close_folder();
        builder.file(b"keep", 5, 5, EntryState::Local);
        builder.file(b"gone", 7, 7, EntryState::Local);
        builder.file(b"swap", 3, 3, EntryState::Local);
        builder.finish().0
    }

    /// x grew to 400, y left, b lost z and gained w 1000, new 20 appeared, and swap is
    /// now a folder.
    fn after() -> Tree {
        let mut builder = TreeBuilder::new("/r".into(), EntryState::Local);
        builder.open_folder(b"a", EntryState::Local);
        builder.file(b"x", 400, 400, EntryState::Local);
        builder.file(b"alias", 0, 0, EntryState::HardLinkAlias);
        builder.close_folder();
        builder.open_folder(b"b", EntryState::Local);
        builder.file(b"w", 1000, 999, EntryState::Local);
        builder.close_folder();
        builder.file(b"keep", 5, 5, EntryState::Local);
        builder.file(b"new", 20, 20, EntryState::Local);
        builder.open_folder(b"swap", EntryState::Local);
        builder.file(b"inner", 2, 2, EntryState::Local);
        builder.close_folder();
        builder.finish().0
    }

    fn rows(changes: &[Change]) -> Vec<(String, Option<u64>, Option<u64>)> {
        changes
            .iter()
            .map(|change| (relative(change, Path::new("/r")), change.old, change.new))
            .collect()
    }

    #[test]
    fn lists_changed_entries_by_depth_largest_first() {
        let (old, new) = (before(), after());
        let s = |text: &str| text.to_owned();
        assert_eq!(
            rows(&entries(&old, &new, 1, Metric::Allocated)),
            [
                (s("b"), Some(10), Some(1000)),
                (s("a"), Some(150), Some(400)),
                (s("new"), None, Some(20)),
                (s("gone"), Some(7), None),
                (s("swap"), Some(3), None),
                (s("swap"), None, Some(2)),
            ]
        );
        assert_eq!(
            rows(&entries(&old, &new, 2, Metric::Allocated)),
            [
                (s("b"), Some(10), Some(1000)),
                (s("b/w"), None, Some(1000)),
                (s("b/z"), Some(10), None),
                (s("a"), Some(150), Some(400)),
                (s("a/x"), Some(100), Some(400)),
                (s("a/y"), Some(50), None),
                (s("a/alias"), None, Some(0)),
                (s("new"), None, Some(20)),
                (s("gone"), Some(7), None),
                (s("swap"), Some(3), None),
                (s("swap"), None, Some(2)),
                (s("swap/inner"), None, Some(2)),
            ]
        );
        assert!(entries(&old, &new, 0, Metric::Allocated).is_empty());
        // Apparent sizes rank by file lengths.
        assert_eq!(entries(&old, &new, 1, Metric::Apparent)[1].old, Some(140));
    }

    #[test]
    fn ranks_files_that_changed_the_most() {
        let (old, new) = (before(), after());
        let s = |text: &str| text.to_owned();
        assert_eq!(
            rows(&files(&old, &new, 3, Metric::Allocated)),
            [
                (s("b/w"), None, Some(1000)),
                (s("a/x"), Some(100), Some(400)),
                (s("a/y"), Some(50), None),
            ]
        );
        assert_eq!(files(&old, &new, 100, Metric::Allocated).len(), 8);
        assert!(files(&old, &new, 0, Metric::Allocated).is_empty());
        assert!(files(&new, &new, 10, Metric::Allocated).is_empty());
    }

    #[test]
    fn prints_signed_changes_and_a_total() {
        let tree = |size| {
            let mut builder = TreeBuilder::new("/r".into(), EntryState::Local);
            builder.file(b"f", size, size, EntryState::Local);
            Report {
                tree: std::sync::Arc::new(builder.finish().0),
                issues: Vec::new(),
                excluded_count: 0,
                is_preview: false,
            }
        };
        let (old, new) = (tree(3_000_000), tree(1_000_000));
        let summary = Summary {
            old: &old,
            new: &new,
            metric: Metric::Allocated,
        };
        let changes = entries(&old.tree, &new.tree, 1, Metric::Allocated);
        let mut text = Vec::new();
        write_plain(&mut text, &summary, &changes).unwrap();
        assert_eq!(
            String::from_utf8(text).unwrap(),
            concat!(
                "     Change        Old        New  Path\n",
                "    -2.0 MB     3.0 MB     1.0 MB  /r/f\n",
                "    -2.0 MB     3.0 MB     1.0 MB  /r  [total]\n",
            )
        );
        let mut json = Vec::new();
        write_json(&mut json, &summary, Selection::Depth(1), &changes).unwrap();
        let json: serde_json::Value = serde_json::from_slice(&json).unwrap();
        assert_eq!(json["changeBytes"], -2_000_000);
        assert_eq!(json["edition"], "pro");
        assert_eq!(json["entries"][0]["oldBytes"], 3_000_000);
        assert_eq!(json["old"]["allocatedBytes"], 3_000_000);
        assert_eq!(signed(0), "0 B");
        assert_eq!(signed(1500), "+2 KB");
    }
}
