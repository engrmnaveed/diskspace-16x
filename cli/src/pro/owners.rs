//! Owners: the space each user's files use, for `scan --by-owner`, which shows who
//! fills a shared server. The scanner keeps each entry's owner in the tree's details
//! side table, read by the same metadata calls as its size.

use std::collections::HashMap;
use std::ffi::CStr;
use std::io::{self, Write};
use std::mem::MaybeUninit;

use serde::Serialize;

use crate::report::{Metric, format_bytes, terminal_path, terminal_text};
use crate::scanner::{Entry, EntryKind, EntryState, Report};

/// The files and folders one user owns below a folder.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnerTotal {
    /// The user's name, or None when the user database has no entry for the ID.
    owner: Option<String>,
    uid: u32,
    allocated_bytes: u64,
    apparent_bytes: u64,
    /// Files and folders, counting each hard-linked file once.
    item_count: u64,
}

impl OwnerTotal {
    /// The owner as text reports show it: the name, or the numeric ID.
    pub(super) fn label(&self) -> String {
        match &self.owner {
            Some(name) => terminal_text(name).into_owned(),
            None => self.uid.to_string(),
        }
    }

    pub(super) fn item_count(&self) -> u64 {
        self.item_count
    }

    pub(super) fn bytes(&self, metric: Metric) -> u64 {
        match metric {
            Metric::Allocated => self.allocated_bytes,
            Metric::Apparent => self.apparent_bytes,
        }
    }
}

/// The owners of the items below `scope`, largest first by `metric`, then by item
/// count and ID. Folders count as items but have no bytes of their own. A hard-link
/// alias counts nowhere, since its file counts at its first path.
pub fn breakdown(
    scope: Entry<'_>,
    metric: Metric,
    name: impl Fn(u32) -> Option<String>,
) -> Vec<OwnerTotal> {
    let mut totals: HashMap<u32, OwnerTotal> = HashMap::new();
    for entry in scope.descendants() {
        if entry.state() == EntryState::HardLinkAlias {
            continue;
        }
        let uid = entry.details().owner();
        let total = totals.entry(uid).or_insert_with(|| OwnerTotal {
            uid,
            ..OwnerTotal::default()
        });
        total.item_count += 1;
        if entry.kind() == EntryKind::File {
            total.allocated_bytes = total
                .allocated_bytes
                .saturating_add(entry.allocated_bytes());
            total.apparent_bytes = total.apparent_bytes.saturating_add(entry.apparent_bytes());
        }
    }
    let mut ranked: Vec<OwnerTotal> = totals
        .into_values()
        .map(|total| OwnerTotal {
            owner: name(total.uid),
            ..total
        })
        .collect();
    ranked.sort_unstable_by(|left, right| {
        right
            .bytes(metric)
            .cmp(&left.bytes(metric))
            .then(right.item_count.cmp(&left.item_count))
            .then(left.uid.cmp(&right.uid))
    });
    ranked
}

/// The user name for `uid` from the user database, or None without an entry. Static
/// Linux builds read `/etc/passwd` only, so directory-service users show their ID.
pub fn user_name(uid: u32) -> Option<String> {
    let mut buffer = vec![0_u8; 1024];
    loop {
        let mut entry = MaybeUninit::<libc::passwd>::uninit();
        let mut result: *mut libc::passwd = std::ptr::null_mut();
        // SAFETY: every pointer is valid for the call, and the buffer outlives the result,
        // whose strings point into it.
        let status = unsafe {
            libc::getpwuid_r(
                uid,
                entry.as_mut_ptr(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                &mut result,
            )
        };
        if status == libc::ERANGE && buffer.len() < 1 << 20 {
            buffer.resize(buffer.len() * 2, 0);
            continue;
        }
        if status != 0 || result.is_null() {
            return None;
        }
        // SAFETY: getpwuid_r succeeded, so pw_name is a C string in the buffer.
        let name = unsafe { CStr::from_ptr((*result).pw_name) };
        return Some(name.to_string_lossy().into_owned()).filter(|name| !name.is_empty());
    }
}

/// A text report of every owner and the total.
pub fn write_plain(out: impl Write, report: &Report, metric: Metric) -> io::Result<()> {
    let mut out = io::BufWriter::new(out);
    let owners = breakdown(report.root(), metric, user_name);
    writeln!(out, "{:>10}  {:>9}  Owner", "Size", "Items")?;
    for owner in &owners {
        writeln!(
            out,
            "{:>10}  {:>9}  {}",
            format_bytes(owner.bytes(metric)),
            owner.item_count,
            owner.label()
        )?;
    }
    let total = if report.issues.is_empty() {
        "[total]"
    } else {
        "[partial total]"
    };
    writeln!(
        out,
        "{:>10}  {:>9}  {}  {total}",
        format_bytes(metric.bytes(report.root())),
        report.root().item_count(),
        terminal_path(report.tree.root_path())
    )?;
    out.flush()
}

/// The part of a JSON report that `--by-owner` adds in place of entries.
#[derive(Serialize)]
pub struct JsonOwners {
    owners: Vec<OwnerTotal>,
}

impl JsonOwners {
    pub fn new(report: &Report, metric: Metric) -> Self {
        Self {
            owners: breakdown(report.root(), metric, user_name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::{Details, TreeBuilder};
    use std::sync::Arc;

    fn report() -> Report {
        let mut builder = TreeBuilder::new("/srv".into(), EntryState::Local);
        let owned = |builder: &mut TreeBuilder, uid| builder.set_details(Details::new(1, uid));
        builder.open_folder(b"alice", EntryState::Local);
        owned(&mut builder, 1001);
        builder.file(b"big", 900, 800, EntryState::Local);
        owned(&mut builder, 1001);
        builder.file(b"shared", 50, 50, EntryState::Local);
        owned(&mut builder, 1002);
        builder.file(b"alias", 0, 0, EntryState::HardLinkAlias);
        owned(&mut builder, 1001);
        builder.close_folder();
        builder.file(b"log", 50, 900, EntryState::Local);
        owned(&mut builder, 0);
        builder.folder(b"empty", EntryState::Local);
        Report {
            tree: Arc::new(builder.finish().0),
            issues: Vec::new(),
            excluded_count: 0,
            is_preview: false,
        }
    }

    fn names(uid: u32) -> Option<String> {
        match uid {
            0 => Some("root".into()),
            1001 => Some("alice".into()),
            _ => None,
        }
    }

    #[test]
    fn sums_bytes_and_items_per_owner() {
        let report = report();
        let owners = breakdown(report.root(), Metric::Allocated, names);
        let summary: Vec<_> = owners
            .iter()
            .map(|owner| (owner.label(), owner.allocated_bytes, owner.item_count))
            .collect();
        // The empty folder has no details set, so it belongs to uid 0, as in a fresh tree.
        assert_eq!(
            summary,
            [
                ("alice".to_string(), 900, 2),
                ("root".to_string(), 50, 2),
                ("1002".to_string(), 50, 1),
            ]
        );
        let apparent = breakdown(report.root(), Metric::Apparent, names);
        assert_eq!(apparent[0].label(), "root");

        let json = serde_json::to_value(&owners[2]).unwrap();
        assert_eq!(json["owner"], serde_json::Value::Null);
        assert_eq!(json["uid"], 1002);
        assert_eq!(json["itemCount"], 1);
    }

    #[test]
    fn writes_owners_and_the_total() {
        let mut text = Vec::new();
        write_plain(&mut text, &report(), Metric::Allocated).unwrap();
        let text = String::from_utf8(text).unwrap();
        let lines: Vec<Vec<&str>> = text
            .lines()
            .map(|line| line.split_whitespace().collect())
            .collect();
        assert_eq!(lines[0], ["Size", "Items", "Owner"]);
        assert_eq!(lines.last().unwrap(), &["1", "KB", "5", "/srv", "[total]"]);
    }

    #[test]
    fn looks_up_the_current_user() {
        // SAFETY: getuid has no preconditions.
        let uid = unsafe { libc::getuid() };
        assert!(user_name(uid).is_some_and(|name| !name.is_empty()));
        assert_eq!(user_name(u32::MAX - 7), None);
    }
}
