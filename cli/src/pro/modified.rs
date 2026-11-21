//! Modification times: `scan --older-than DAYS` and the dates the explorer shows.
//!
//! The scanner keeps each file's modification time in the tree's details side table,
//! and a folder's time is the newest of any file below it.

use std::mem::MaybeUninit;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::scanner::{EntryKind, EntryState, NodeId, Report};

const SECONDS_PER_DAY: u64 = 86_400;

/// The time `days` days before now, in seconds since 1970.
pub fn cutoff(days: u32, now: SystemTime) -> u32 {
    let now = now
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    let cutoff = now.saturating_sub(u64::from(days) * SECONDS_PER_DAY);
    u32::try_from(cutoff).unwrap_or(u32::MAX)
}

/// The report with only the files last modified before `cutoff`, and the folders that
/// hold them. Folder totals, item counts and times count those files alone. Folders that
/// couldn't be read completely stay, so their state still shows.
pub fn older_than(report: Report, cutoff: u32) -> Report {
    let tree = &report.tree;
    let mut removed: Vec<NodeId> = Vec::new();
    let root = tree.root();
    // Open folders, each with its remaining children and whether it keeps anything.
    let mut stack = vec![(root, root.children(), false)];
    while let Some((_, children, keeps)) = stack.last_mut() {
        match children.next() {
            Some(child) if child.kind() == EntryKind::File => {
                if child.details().modified().is_some_and(|time| time < cutoff) {
                    *keeps = true;
                } else {
                    removed.push(child.id());
                }
            }
            Some(child) => stack.push((child, child.children(), false)),
            None => {
                let (folder, _, keeps) = stack.pop().expect("the stack isn't empty");
                let Some(parent) = stack.last_mut() else {
                    break;
                };
                if keeps || folder.own_state() != EntryState::Local {
                    parent.2 = true;
                } else {
                    removed.push(folder.id());
                }
            }
        }
    }
    if removed.is_empty() {
        return report;
    }
    Report {
        tree: Arc::new(tree.without(&removed)),
        ..report
    }
}

/// A modification time as a local date, such as 2026-11-21.
pub fn format_date(seconds: u32) -> String {
    calendar(u64::from(seconds), true).map_or_else(
        || "-".into(),
        |parts| {
            format!(
                "{:04}-{:02}-{:02}",
                parts.tm_year + 1900,
                parts.tm_mon + 1,
                parts.tm_mday
            )
        },
    )
}

/// A time as a local date and minute, such as 2026-11-21 16:15.
pub fn format_time(seconds: u64) -> String {
    calendar(seconds, true).map_or_else(
        || "-".into(),
        |parts| {
            format!(
                "{:04}-{:02}-{:02} {:02}:{:02}",
                parts.tm_year + 1900,
                parts.tm_mon + 1,
                parts.tm_mday,
                parts.tm_hour,
                parts.tm_min
            )
        },
    )
}

/// A time in UTC for file names, such as 20261121T161500Z.
pub fn utc_stamp(seconds: u64) -> String {
    calendar(seconds, false).map_or_else(
        || "00000000T000000Z".into(),
        |parts| {
            format!(
                "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
                parts.tm_year + 1900,
                parts.tm_mon + 1,
                parts.tm_mday,
                parts.tm_hour,
                parts.tm_min,
                parts.tm_sec
            )
        },
    )
}

/// The calendar fields of a time, in local time or UTC.
fn calendar(seconds: u64, local: bool) -> Option<libc::tm> {
    let time = libc::time_t::try_from(seconds).ok()?;
    let mut parts = MaybeUninit::<libc::tm>::zeroed();
    // SAFETY: both pointers are valid, and the call writes only into `parts`.
    let result = unsafe {
        if local {
            libc::localtime_r(&time, parts.as_mut_ptr())
        } else {
            libc::gmtime_r(&time, parts.as_mut_ptr())
        }
    };
    // SAFETY: the call succeeded and filled in the struct.
    (!result.is_null()).then(|| unsafe { parts.assume_init() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::{Details, TreeBuilder};
    use std::path::Path;
    use std::time::Duration;

    const DAY: i64 = SECONDS_PER_DAY as i64;
    const NOW: i64 = 1_800_000_000;

    fn report() -> Report {
        let mut builder = TreeBuilder::new("/r".into(), EntryState::Local);
        let file = |builder: &mut TreeBuilder, name: &[u8], bytes, age: i64| {
            builder.file(name, bytes, bytes, EntryState::Local);
            builder.set_details(Details::new(NOW - age * DAY, 501));
        };
        builder.open_folder(b"old", EntryState::Local);
        file(&mut builder, b"a", 100, 400);
        file(&mut builder, b"b", 50, 30);
        builder.close_folder();
        builder.open_folder(b"new", EntryState::Local);
        file(&mut builder, b"c", 70, 2);
        builder.close_folder();
        builder.open_folder(b"locked", EntryState::Protected);
        builder.close_folder();
        builder.open_folder(b"empty", EntryState::Local);
        builder.close_folder();
        file(&mut builder, b"top", 9, 90);
        Report {
            tree: Arc::new(builder.finish().0),
            issues: Vec::new(),
            excluded_count: 0,
            is_preview: false,
        }
    }

    #[test]
    fn folders_show_their_newest_file() {
        let report = report();
        let tree = &report.tree;
        let time = |path: &str| tree.entry(tree.find(Path::new(path)).unwrap()).details();
        assert_eq!(time("/r/old").modified(), Some((NOW - 30 * DAY) as u32));
        assert_eq!(time("/r/old").owner(), 0);
        assert_eq!(time("/r/old/a").owner(), 501);
        assert_eq!(time("/r/empty").modified(), None);
        assert_eq!(
            tree.root().details().modified(),
            Some((NOW - 2 * DAY) as u32)
        );
    }

    #[test]
    fn keeps_only_files_older_than_the_cutoff() {
        let now = UNIX_EPOCH + Duration::from_secs(NOW as u64);
        let narrowed = older_than(report(), cutoff(60, now));
        let tree = &narrowed.tree;
        let names: Vec<_> = tree
            .root()
            .descendants()
            .map(|entry| tree.relative_path(entry.id(), NodeId::ROOT))
            .collect();
        assert_eq!(
            names,
            [
                Path::new("old"),
                Path::new("old/a"),
                Path::new("locked"),
                Path::new("top")
            ]
        );
        assert_eq!(tree.root().allocated_bytes(), 109);
        assert_eq!(tree.root().item_count(), 4);
        // The folder's time now comes from the files that remain.
        let old = tree.entry(tree.find(Path::new("/r/old")).unwrap());
        assert_eq!(old.details().modified(), Some((NOW - 400 * DAY) as u32));
        assert_eq!(old.allocated_bytes(), 100);

        let everything = older_than(report(), cutoff(0, now + Duration::from_secs(1)));
        assert_eq!(everything.tree.root().allocated_bytes(), 229);
        let nothing = older_than(report(), cutoff(10_000, now));
        assert_eq!(nothing.tree.root().allocated_bytes(), 0);
        assert_eq!(nothing.tree.root().item_count(), 1);
    }

    #[test]
    fn formats_local_dates() {
        // Noon UTC on 2026-07-01 is that date or a neighbor in every time zone.
        let date = format_date(1_782_907_200);
        assert!(
            ["2026-06-30", "2026-07-01", "2026-07-02"].contains(&date.as_str()),
            "{date}"
        );
        assert_eq!(utc_stamp(1_782_907_200), "20260701T120000Z");
        assert!(format_time(1_782_907_200).starts_with("2026-0"));
        assert_eq!(cutoff(1, UNIX_EPOCH + Duration::from_secs(100)), 0);
        assert_eq!(
            cutoff(1, UNIX_EPOCH + Duration::from_secs(200_000)),
            200_000 - 86_400
        );
    }
}
