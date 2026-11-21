//! Cleanup suggestions: folders that tools recreate when they need them, such as
//! `node_modules`, Cargo's `target` or Xcode's DerivedData, found by name in a finished
//! scan. It reads nothing new and removes nothing. The explorer's `u` view and the Mac
//! app hand the folders a user picks to the confirmed Move to Trash.

use std::io::{self, Write};
use std::path::Path;

use serde::Serialize;

use crate::report::{JsonPath, Metric, format_bytes, terminal_path};
use crate::scanner::{Entry, EntryKind, EntryState, NodeId, Report};

/// The smallest folder worth suggesting, in allocated bytes.
pub const SMALLEST: u64 = 1_000_000;

/// A kind of folder that a tool recreates.
pub struct Rule {
    /// A stable name for JSON, as `node-modules`.
    pub id: &'static str,
    /// What the folder holds, as reports show it.
    pub label: &'static str,
    kind: Kind,
}

enum Kind {
    /// A folder with this name anywhere.
    Name(&'static str),
    /// A folder with this name beside a file named like the second.
    Beside(&'static str, &'static str),
    /// A folder whose path ends with these components.
    PathEnd(&'static str),
}

/// The rules, in the order they're tried. `fixtures/cleanup` holds a scan both apps test.
pub const RULES: &[Rule] = &[
    Rule {
        id: "xcode-derived-data",
        label: "Xcode build data",
        kind: Kind::PathEnd("Library/Developer/Xcode/DerivedData"),
    },
    Rule {
        id: "xcode-device-support",
        label: "Xcode device support",
        kind: Kind::PathEnd("Library/Developer/Xcode/iOS DeviceSupport"),
    },
    Rule {
        id: "simulator-caches",
        label: "Simulator caches",
        kind: Kind::PathEnd("Library/Developer/CoreSimulator/Caches"),
    },
    Rule {
        id: "app-caches",
        label: "App caches",
        kind: Kind::PathEnd("Library/Caches"),
    },
    Rule {
        id: "gradle-caches",
        label: "Gradle caches",
        kind: Kind::PathEnd(".gradle/caches"),
    },
    Rule {
        id: "npm-cache",
        label: "npm cache",
        kind: Kind::PathEnd(".npm/_cacache"),
    },
    Rule {
        id: "cache-folder",
        label: "Cache folder",
        kind: Kind::Name(".cache"),
    },
    Rule {
        id: "node-modules",
        label: "Node packages",
        kind: Kind::Name("node_modules"),
    },
    Rule {
        id: "rust-target",
        label: "Rust build output",
        kind: Kind::Beside("target", "Cargo.toml"),
    },
];

impl Rule {
    fn matches(&self, folder: Entry<'_>, child: Entry<'_>) -> bool {
        let name = child.name();
        match self.kind {
            Kind::Name(wanted) => name == wanted,
            Kind::Beside(wanted, marker) => {
                name == wanted
                    && folder.children().any(|sibling| {
                        sibling.kind() == EntryKind::File && sibling.name() == marker
                    })
            }
            Kind::PathEnd(suffix) => {
                Path::new(suffix).file_name() == Some(name) && child.path().ends_with(suffix)
            }
        }
    }
}

/// The rule that the folder `child` of `folder` matches, if any.
pub fn rule_for(folder: Entry<'_>, child: Entry<'_>) -> Option<&'static Rule> {
    if child.kind() != EntryKind::Directory {
        return None;
    }
    RULES.iter().find(|rule| rule.matches(folder, child))
}

pub struct Suggestion {
    pub id: NodeId,
    pub rule: &'static Rule,
}

/// Folders below `scope` that a rule matches, read completely and at least 1 MB, largest
/// first by `metric` with ties in path order. A matched folder's contents aren't searched,
/// so `node_modules` inside `node_modules` isn't listed again.
pub fn suggestions(scope: Entry<'_>, metric: Metric) -> Vec<Suggestion> {
    let mut found = Vec::new();
    let mut folders = vec![scope];
    while let Some(folder) = folders.pop() {
        for child in folder.children() {
            if child.kind() != EntryKind::Directory {
                continue;
            }
            match rule_for(folder, child) {
                Some(rule) => {
                    if child.state() == EntryState::Local && child.allocated_bytes() >= SMALLEST {
                        found.push(Suggestion {
                            id: child.id(),
                            rule,
                        });
                    }
                }
                None => folders.push(child),
            }
        }
    }
    let tree = scope.tree();
    found.sort_by(|left, right| {
        let bytes = |suggestion: &Suggestion| metric.bytes(tree.entry(suggestion.id));
        bytes(right)
            .cmp(&bytes(left))
            .then_with(|| tree.cmp_paths(left.id, right.id))
    });
    found
}

/// The text report: each suggestion with its size and kind, then the total.
pub fn write_plain(out: impl Write, report: &Report, metric: Metric) -> io::Result<()> {
    let mut out = io::BufWriter::new(out);
    let found = suggestions(report.root(), metric);
    writeln!(out, "{:>10}  {:<20}  Path", "Size", "Suggestion")?;
    let mut total = 0_u64;
    for suggestion in &found {
        let entry = report.tree.entry(suggestion.id);
        total = total.saturating_add(metric.bytes(entry));
        writeln!(
            out,
            "{:>10}  {:<20}  {}",
            format_bytes(metric.bytes(entry)),
            suggestion.rule.label,
            terminal_path(&entry.path())
        )?;
    }
    let count = match found.len() {
        1 => "1 suggestion".to_owned(),
        count => format!("{count} suggestions"),
    };
    writeln!(out, "{:>10}  {count:<20}  [total]", format_bytes(total))?;
    out.flush()
}

/// The part of a JSON report that `--cleanup` adds in place of entries.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonCleanup {
    cleanup_bytes: u64,
    suggestions: Vec<JsonSuggestion>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonSuggestion {
    #[serde(flatten)]
    path: JsonPath<'static>,
    rule: &'static str,
    label: &'static str,
    allocated_bytes: u64,
    apparent_bytes: u64,
    item_count: u64,
}

impl JsonCleanup {
    pub fn new(report: &Report, metric: Metric) -> Self {
        let suggestions: Vec<JsonSuggestion> = suggestions(report.root(), metric)
            .into_iter()
            .map(|suggestion| {
                let entry = report.tree.entry(suggestion.id);
                JsonSuggestion {
                    path: JsonPath::owned(&entry.path()),
                    rule: suggestion.rule.id,
                    label: suggestion.rule.label,
                    allocated_bytes: entry.allocated_bytes(),
                    apparent_bytes: entry.apparent_bytes(),
                    item_count: entry.item_count(),
                }
            })
            .collect();
        Self {
            cleanup_bytes: suggestions.iter().fold(0, |total, suggestion| {
                total.saturating_add(match metric {
                    Metric::Allocated => suggestion.allocated_bytes,
                    Metric::Apparent => suggestion.apparent_bytes,
                })
            }),
            suggestions,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Expected {
        minimum_bytes: u64,
        suggestions: Vec<ExpectedSuggestion>,
    }

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ExpectedSuggestion {
        path: String,
        rule: String,
        allocated_bytes: u64,
    }

    fn fixture() -> Report {
        let mut input: &[u8] = include_bytes!("../../../fixtures/cleanup/scan.json");
        crate::ncdu::read(&mut |buffer| io::Read::read(&mut input, buffer))
            .unwrap()
            .report
    }

    /// `CleanupSuggestionsTests` in the Mac app reads the same scan and expectations.
    #[test]
    fn matches_the_shared_cleanup_fixture() {
        let expected: Expected =
            serde_json::from_str(include_str!("../../../fixtures/cleanup/expected.json")).unwrap();
        assert_eq!(expected.minimum_bytes, SMALLEST);
        let report = fixture();
        let found: Vec<(String, &str, u64)> = suggestions(report.root(), Metric::Allocated)
            .into_iter()
            .map(|suggestion| {
                let entry = report.tree.entry(suggestion.id);
                (
                    entry.path().to_string_lossy().into_owned(),
                    suggestion.rule.id,
                    entry.allocated_bytes(),
                )
            })
            .collect();
        let wanted: Vec<(String, &str, u64)> = expected
            .suggestions
            .iter()
            .map(|item| (item.path.clone(), item.rule.as_str(), item.allocated_bytes))
            .collect();
        assert_eq!(found, wanted);
        // Below a folder, only its own matches are listed.
        let projects = report
            .tree
            .find(Path::new("/Users/me/Projects"))
            .expect("the fixture has Projects");
        let below: Vec<&str> = suggestions(report.tree.entry(projects), Metric::Allocated)
            .iter()
            .map(|suggestion| suggestion.rule.id)
            .collect();
        assert_eq!(below, ["rust-target", "node-modules"]);
    }

    #[test]
    fn prints_suggestions_and_their_total() {
        let report = fixture();
        let mut plain = Vec::new();
        write_plain(&mut plain, &report, Metric::Allocated).unwrap();
        let plain = String::from_utf8(plain).unwrap();
        let lines: Vec<&str> = plain.lines().collect();
        assert_eq!(lines[0], "      Size  Suggestion            Path");
        assert_eq!(
            lines[1],
            "    7.0 MB  App caches            /Users/me/Library/Caches"
        );
        assert_eq!(
            lines.last(),
            Some(&"   26.1 MB  9 suggestions         [total]")
        );

        let json = serde_json::to_value(JsonCleanup::new(&report, Metric::Allocated)).unwrap();
        assert_eq!(json["cleanupBytes"], 26_100_000);
        assert_eq!(json["suggestions"][0]["path"], "/Users/me/Library/Caches");
        assert_eq!(json["suggestions"][0]["rule"], "app-caches");
        assert_eq!(json["suggestions"][0]["label"], "App caches");
        assert_eq!(json["suggestions"][0]["itemCount"], 4);
        assert_eq!(json["suggestions"].as_array().unwrap().len(), 9);
    }
}
