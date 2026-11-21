//! File types: the space each file extension uses, for `scan --types N` and the
//! explorer's `t` view. Types come from names alone, so this reads nothing new.

use std::collections::HashMap;
use std::io::{self, Write};

use serde::Serialize;

use crate::report::{Metric, format_bytes, terminal_path, terminal_text};
use crate::scanner::{Entry, EntryKind, EntryState, Report};

/// The longest extension that counts as a type. Longer ones, as in
/// `backup.2026-11-21T10-00-00`, are usually part of the name.
const LONGEST_EXTENSION: usize = 16;

/// The type of a file named `name`: the text after its last dot in ASCII lowercase, or
/// an empty string when it has none. A leading dot, as in `.bashrc`, starts a name rather
/// than an extension, and an extension can't be empty, contain a space, or be longer
/// than 16 bytes. `fixtures/file-types.json` holds the cases both apps test.
pub fn type_of(name: &[u8]) -> String {
    let Some(dot) = name.iter().rposition(|&byte| byte == b'.') else {
        return String::new();
    };
    let extension = &name[dot + 1..];
    if dot == 0
        || extension.is_empty()
        || extension.len() > LONGEST_EXTENSION
        || extension.contains(&b' ')
    {
        return String::new();
    }
    String::from_utf8_lossy(extension).to_ascii_lowercase()
}

/// The files of one type below a folder.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeTotal {
    /// The extension without its dot, or empty for files without one.
    #[serde(rename = "type")]
    pub name: String,
    pub allocated_bytes: u64,
    pub apparent_bytes: u64,
    pub file_count: u64,
}

impl TypeTotal {
    /// The type as text reports show it: `.mov`, or `(no extension)`.
    pub fn label(&self) -> String {
        if self.name.is_empty() {
            "(no extension)".into()
        } else {
            format!(".{}", terminal_text(&self.name))
        }
    }
}

/// The types of the files below `scope`, largest first by `metric`, with ties in name
/// order. Each hard-linked file counts once, at the path the scan counted.
pub fn breakdown(scope: Entry<'_>, metric: Metric) -> Vec<TypeTotal> {
    let mut totals: HashMap<String, TypeTotal> = HashMap::new();
    for entry in scope.descendants() {
        if entry.kind() != EntryKind::File || entry.state() == EntryState::HardLinkAlias {
            continue;
        }
        let name = type_of(entry.name().as_encoded_bytes());
        let total = totals.entry(name).or_default();
        total.allocated_bytes = total
            .allocated_bytes
            .saturating_add(entry.allocated_bytes());
        total.apparent_bytes = total.apparent_bytes.saturating_add(entry.apparent_bytes());
        total.file_count += 1;
    }
    let mut ranked: Vec<TypeTotal> = totals
        .into_iter()
        .map(|(name, total)| TypeTotal { name, ..total })
        .collect();
    let bytes = |total: &TypeTotal| match metric {
        Metric::Allocated => total.allocated_bytes,
        Metric::Apparent => total.apparent_bytes,
    };
    ranked.sort_unstable_by(|left, right| {
        bytes(right)
            .cmp(&bytes(left))
            .then_with(|| left.name.cmp(&right.name))
    });
    ranked
}

/// A text report of the `limit` largest types, the rest in one line, and the total.
pub fn write_plain(
    out: impl Write,
    report: &Report,
    limit: usize,
    metric: Metric,
) -> io::Result<()> {
    let mut out = io::BufWriter::new(out);
    let types = breakdown(report.root(), metric);
    let bytes = |total: &TypeTotal| match metric {
        Metric::Allocated => total.allocated_bytes,
        Metric::Apparent => total.apparent_bytes,
    };
    writeln!(out, "{:>10}  {:>9}  Type", "Size", "Files")?;
    for total in types.iter().take(limit) {
        writeln!(
            out,
            "{:>10}  {:>9}  {}",
            format_bytes(bytes(total)),
            total.file_count,
            total.label()
        )?;
    }
    let rest = types.get(limit..).unwrap_or_default();
    if !rest.is_empty() {
        let (size, files) = rest.iter().fold((0_u64, 0_u64), |(size, files), total| {
            (size.saturating_add(bytes(total)), files + total.file_count)
        });
        let kinds = if rest.len() == 1 { "type" } else { "types" };
        writeln!(
            out,
            "{:>10}  {files:>9}  ({} other {kinds})",
            format_bytes(size),
            rest.len()
        )?;
    }
    let files: u64 = types.iter().map(|total| total.file_count).sum();
    let total = if report.issues.is_empty() {
        "[total]"
    } else {
        "[partial total]"
    };
    writeln!(
        out,
        "{:>10}  {files:>9}  {}  {total}",
        format_bytes(metric.bytes(report.root())),
        terminal_path(report.tree.root_path())
    )?;
    out.flush()
}

/// The part of a JSON report that `--types N` adds in place of entries.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonTypes {
    type_limit: usize,
    /// Distinct types in the whole scan, including those past the limit.
    type_count: usize,
    types: Vec<TypeTotal>,
}

impl JsonTypes {
    pub fn new(report: &Report, limit: usize, metric: Metric) -> Self {
        let mut types = breakdown(report.root(), metric);
        let type_count = types.len();
        types.truncate(limit);
        Self {
            type_limit: limit,
            type_count,
            types,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::TreeBuilder;
    use std::sync::Arc;

    #[test]
    fn matches_the_shared_type_table() {
        #[derive(serde::Deserialize)]
        struct Case {
            name: String,
            #[serde(rename = "type")]
            kind: String,
        }
        #[derive(serde::Deserialize)]
        struct Table {
            names: Vec<Case>,
        }
        let table: Table =
            serde_json::from_str(include_str!("../../../fixtures/file-types.json")).unwrap();
        assert!(!table.names.is_empty());
        for case in table.names {
            assert_eq!(type_of(case.name.as_bytes()), case.kind, "{}", case.name);
        }
        assert_eq!(type_of(b"bad\xff.Bin"), "bin");
    }

    #[test]
    fn ranks_types_by_size_and_counts_hard_links_once() {
        let mut builder = TreeBuilder::new("/r".into(), EntryState::Local);
        builder.open_folder(b"clips", EntryState::Local);
        builder.file(b"a.MOV", 300, 290, EntryState::Local);
        builder.file(b"b.mov", 200, 210, EntryState::Local);
        builder.file(b"alias.mov", 0, 0, EntryState::HardLinkAlias);
        builder.close_folder();
        builder.file(b"notes.txt", 5, 4, EntryState::Local);
        builder.file(b"Makefile", 5, 900, EntryState::Local);
        builder.file(b"cloud.txt", 0, 0, EntryState::CloudOnly);
        let report = Report {
            tree: Arc::new(builder.finish().0),
            issues: Vec::new(),
            excluded_count: 0,
            is_preview: false,
        };
        let types = breakdown(report.root(), Metric::Allocated);
        let summary: Vec<_> = types
            .iter()
            .map(|total| (total.name.as_str(), total.allocated_bytes, total.file_count))
            .collect();
        assert_eq!(summary, [("mov", 500, 2), ("", 5, 1), ("txt", 5, 2)]);
        let apparent = breakdown(report.root(), Metric::Apparent);
        assert_eq!(apparent[0].name, "");
        assert_eq!(types[1].label(), "(no extension)");

        let mut text = Vec::new();
        write_plain(&mut text, &report, 1, Metric::Allocated).unwrap();
        let text = String::from_utf8(text).unwrap();
        let lines: Vec<_> = text
            .lines()
            .map(str::split_whitespace)
            .map(Iterator::collect::<Vec<_>>)
            .collect();
        assert_eq!(lines[0], ["Size", "Files", "Type"]);
        assert_eq!(lines[1], ["500", "B", "2", ".mov"]);
        assert_eq!(lines[2], ["10", "B", "3", "(2", "other", "types)"]);
        assert_eq!(lines[3], ["510", "B", "5", "/r", "[total]"]);

        let json = serde_json::to_value(JsonTypes::new(&report, 2, Metric::Allocated)).unwrap();
        assert_eq!(json["typeLimit"], 2);
        assert_eq!(json["typeCount"], 3);
        assert_eq!(json["types"][0]["type"], "mov");
        assert_eq!(json["types"][0]["fileCount"], 2);
        assert_eq!(json["types"][1]["type"], "");
        assert_eq!(json["types"].as_array().unwrap().len(), 2);
    }
}
