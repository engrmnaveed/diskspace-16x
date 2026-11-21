//! Exclusive sizes for `scan --exclusive`: the space that moving an item to the Trash and
//! emptying it frees at least. Blocks a file shares with APFS clones or Btrfs and XFS
//! reflinks don't count, and neither does a file with other hard links.
//!
//! After the scan, one metadata call per counted file reads how much of it is shared,
//! on the scan's threads. Only files that share blocks are kept, with their folders'
//! totals, so the table stays small. It reads no file contents.

use std::collections::HashMap;
use std::io::{self, Write};
use std::path::Path;

use serde::Serialize;

use crate::report::{self, JsonPath, Selection, format_bytes, terminal_path};
use crate::scanner::{Entry, EntryKind, EntryState, NodeId, Report, Tree};

pub struct Exclusive {
    /// Shared bytes of each file that shares blocks, and the totals of the folders above.
    shared: HashMap<NodeId, u64>,
}

impl Exclusive {
    /// Reads how much of each counted file in `tree` is shared, on `threads` threads.
    pub fn measure(tree: &Tree, threads: usize) -> Self {
        let files: Vec<NodeId> = tree
            .root()
            .descendants()
            .filter(|entry| {
                entry.kind() == EntryKind::File
                    && entry.state() == EntryState::Local
                    && entry.allocated_bytes() > 0
            })
            .map(Entry::id)
            .collect();
        let sharing = Sharing::of(tree.root_path());
        let chunk = files.len().div_ceil(threads.max(1)).max(1);
        let shared_files: Vec<(NodeId, u64)> = std::thread::scope(|scope| {
            let workers: Vec<_> = files
                .chunks(chunk)
                .map(|part| {
                    scope.spawn(move || {
                        part.iter()
                            .filter_map(|&id| {
                                let entry = tree.entry(id);
                                let bytes = sharing
                                    .shared_bytes(&entry.path())
                                    .min(entry.allocated_bytes());
                                (bytes > 0).then_some((id, bytes))
                            })
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            workers
                .into_iter()
                .flat_map(|worker| worker.join().unwrap_or_default())
                .collect()
        });
        let mut shared = HashMap::new();
        for (id, bytes) in shared_files {
            let mut current = Some(tree.entry(id));
            while let Some(entry) = current {
                let total = shared.entry(entry.id()).or_insert(0_u64);
                *total = total.saturating_add(bytes);
                current = entry.parent();
            }
        }
        Self { shared }
    }

    /// The bytes that removing `entry` frees at least.
    pub fn bytes(&self, entry: Entry<'_>) -> u64 {
        let shared = self.shared.get(&entry.id()).copied().unwrap_or(0);
        entry.allocated_bytes().saturating_sub(shared)
    }
}

/// How this filesystem can share a file's blocks.
#[derive(Clone, Copy)]
struct Sharing {
    /// Btrfs, XFS and bcachefs share extents between reflinked files.
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    reflinks: bool,
}

impl Sharing {
    #[cfg(target_os = "macos")]
    fn of(_root: &Path) -> Self {
        Self { reflinks: false }
    }

    #[cfg(target_os = "linux")]
    fn of(root: &Path) -> Self {
        use std::os::unix::ffi::OsStrExt;
        const BTRFS: u32 = 0x9123_683e;
        const XFS: u32 = 0x5846_5342;
        const BCACHEFS: u32 = 0xca45_1a4e;
        let Ok(path) = std::ffi::CString::new(root.as_os_str().as_bytes()) else {
            return Self { reflinks: false };
        };
        let mut stats = std::mem::MaybeUninit::<libc::statfs>::uninit();
        // SAFETY: the path is a C string, and statfs fills the buffer when it returns 0.
        let reflinks = unsafe { libc::statfs(path.as_ptr(), stats.as_mut_ptr()) } == 0 && {
            // SAFETY: statfs returned 0. Magic numbers fit in 32 bits on every target.
            #[allow(clippy::unnecessary_cast)]
            let kind = unsafe { stats.assume_init_ref() }.f_type as u32;
            matches!(kind, BTRFS | XFS | BCACHEFS)
        };
        Self { reflinks }
    }

    /// The bytes of the file at `path` that removing it wouldn't free: all of them when
    /// it has other hard links, else the blocks it shares with clones. Zero when unknown.
    #[cfg(target_os = "macos")]
    fn shared_bytes(self, path: &Path) -> u64 {
        use std::os::unix::ffi::OsStrExt;
        let Ok(path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
            return 0;
        };
        let mut request = libc::attrlist {
            bitmapcount: libc::ATTR_BIT_MAP_COUNT,
            reserved: 0,
            commonattr: libc::ATTR_CMN_RETURNED_ATTRS,
            volattr: 0,
            dirattr: 0,
            fileattr: libc::ATTR_FILE_LINKCOUNT | libc::ATTR_FILE_ALLOCSIZE,
            forkattr: libc::ATTR_CMNEXT_PRIVATESIZE,
        };
        // The length, the returned attributes, the link count, the allocated size and the
        // private size, each packed at 4-byte alignment.
        let mut buffer = [0_u8; 48];
        // SAFETY: the request and buffer outlive the call, which writes at most
        // `buffer.len()` bytes.
        let status = unsafe {
            libc::getattrlist(
                path.as_ptr(),
                (&raw mut request).cast(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                (libc::FSOPT_NOFOLLOW
                    | libc::FSOPT_ATTR_CMN_EXTENDED
                    | libc::FSOPT_PACK_INVAL_ATTRS) as _,
            )
        };
        if status != 0 {
            return 0;
        }
        let u32_at = |at: usize| u32::from_ne_bytes(buffer[at..at + 4].try_into().unwrap());
        let i64_at = |at: usize| i64::from_ne_bytes(buffer[at..at + 8].try_into().unwrap());
        // attribute_set_t: common, volume, directory, file and fork groups.
        let returned_fork = u32_at(4 + 16);
        let links = u32_at(24);
        let allocated = i64_at(28).max(0) as u64;
        if links > 1 {
            return allocated;
        }
        if returned_fork & libc::ATTR_CMNEXT_PRIVATESIZE == 0 {
            return 0;
        }
        allocated.saturating_sub(i64_at(36).max(0) as u64)
    }

    #[cfg(target_os = "linux")]
    fn shared_bytes(self, path: &Path) -> u64 {
        use std::os::unix::fs::MetadataExt;
        let Ok(metadata) = std::fs::symlink_metadata(path) else {
            return 0;
        };
        let allocated = metadata.blocks().saturating_mul(512);
        if metadata.nlink() > 1 {
            return allocated;
        }
        if !self.reflinks {
            return 0;
        }
        linux::shared_extent_bytes(path).unwrap_or(0)
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::fs::File;
    use std::io;
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::OpenOptionsExt;
    use std::path::Path;

    /// `_IOWR('f', 11, struct fiemap)` from `<linux/fs.h>`.
    const FS_IOC_FIEMAP: libc::c_ulong = 0xc020_660b;
    const FIEMAP_EXTENT_LAST: u32 = 0x1;
    const FIEMAP_EXTENT_SHARED: u32 = 0x2000;
    const EXTENTS: usize = 64;

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Extent {
        logical: u64,
        physical: u64,
        length: u64,
        reserved64: [u64; 2],
        flags: u32,
        reserved: [u32; 3],
    }

    #[repr(C)]
    struct Map {
        start: u64,
        length: u64,
        flags: u32,
        mapped_extents: u32,
        extent_count: u32,
        reserved: u32,
        extents: [Extent; EXTENTS],
    }

    /// The bytes of the file's extents that `FIEMAP` marks as shared with another file.
    /// Opening a file to map it reads no contents.
    pub fn shared_extent_bytes(path: &Path) -> io::Result<u64> {
        let file = File::options()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_NOCTTY)
            .open(path)?;
        let mut shared = 0_u64;
        let mut start = 0_u64;
        loop {
            let mut map = Map {
                start,
                length: u64::MAX - start,
                flags: 0,
                mapped_extents: 0,
                extent_count: EXTENTS as u32,
                reserved: 0,
                extents: [Extent::default(); EXTENTS],
            };
            // SAFETY: the descriptor is open, and the map has room for `extent_count`
            // extents after its header, as FS_IOC_FIEMAP expects.
            if unsafe { libc::ioctl(file.as_raw_fd(), FS_IOC_FIEMAP as _, &raw mut map) } < 0 {
                return Err(io::Error::last_os_error());
            }
            let mapped = &map.extents[..(map.mapped_extents as usize).min(EXTENTS)];
            let Some(last) = mapped.last() else {
                return Ok(shared);
            };
            for extent in mapped {
                if extent.flags & FIEMAP_EXTENT_SHARED != 0 {
                    shared = shared.saturating_add(extent.length);
                }
            }
            if last.flags & FIEMAP_EXTENT_LAST != 0 {
                return Ok(shared);
            }
            start = last.logical.saturating_add(last.length);
        }
    }
}

/// The text report for `--exclusive`: the usual entries or top files, with each one's
/// allocated and exclusive bytes.
pub fn write_plain(
    out: impl Write,
    report: &Report,
    selection: Selection,
    exclusive: &Exclusive,
) -> io::Result<()> {
    let mut out = io::BufWriter::new(out);
    let tree = &report.tree;
    writeln!(out, "{:>10}  {:>10}  Path", "Size", "Exclusive")?;
    for id in report::selected_entries(tree, selection, report::Metric::Allocated) {
        let entry = tree.entry(id);
        writeln!(
            out,
            "{:>10}  {:>10}  {}",
            format_bytes(entry.allocated_bytes()),
            format_bytes(exclusive.bytes(entry)),
            terminal_path(&entry.path())
        )?;
    }
    let total = if report.issues.is_empty() {
        "[total]"
    } else {
        "[partial total]"
    };
    let root = report.root();
    writeln!(
        out,
        "{:>10}  {:>10}  {}  {total}",
        format_bytes(root.allocated_bytes()),
        format_bytes(exclusive.bytes(root)),
        terminal_path(tree.root_path())
    )?;
    out.flush()
}

/// The part of a JSON report that `--exclusive` writes in place of the usual entries.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonExclusive {
    exclusive_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    depth: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    entries: Option<Vec<JsonEntry>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_limit: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_files: Option<Vec<JsonEntry>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonEntry {
    #[serde(flatten)]
    path: JsonPath<'static>,
    allocated_bytes: u64,
    apparent_bytes: u64,
    exclusive_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    kind: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    item_count: Option<u64>,
}

impl JsonExclusive {
    pub fn new(report: &Report, selection: Selection, exclusive: &Exclusive) -> Self {
        let tree = &report.tree;
        let depth = match selection {
            Selection::Depth(depth) => Some(depth),
            Selection::Top(_) => None,
        };
        let rows: Vec<JsonEntry> =
            report::selected_entries(tree, selection, report::Metric::Allocated)
                .into_iter()
                .map(|id| {
                    let entry = tree.entry(id);
                    let directory = entry.kind() == EntryKind::Directory;
                    JsonEntry {
                        path: JsonPath::owned(&entry.path()),
                        allocated_bytes: entry.allocated_bytes(),
                        apparent_bytes: entry.apparent_bytes(),
                        exclusive_bytes: exclusive.bytes(entry),
                        kind: depth.map(|_| if directory { "directory" } else { "file" }),
                        item_count: depth.and(directory.then(|| entry.item_count())),
                    }
                })
                .collect();
        let (entries, top_files, top_limit) = match selection {
            Selection::Depth(_) => (Some(rows), None, None),
            Selection::Top(limit) => (None, Some(rows), Some(limit)),
        };
        Self {
            exclusive_bytes: exclusive.bytes(report.root()),
            depth,
            entries,
            top_limit,
            top_files,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn scan(root: &Path) -> Report {
        crate::scanner::scan(
            root,
            crate::scanner::Options {
                excludes: &crate::scanner::Excludes::new(Vec::<String>::new()).unwrap(),
                threads: 2,
            },
            false,
            &std::sync::atomic::AtomicBool::new(false),
            |_| {},
        )
        .unwrap()
    }

    fn fixture(name: &str) -> std::path::PathBuf {
        let root =
            std::env::temp_dir().join(format!("space16x-exclusive-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("own")).unwrap();
        std::fs::create_dir_all(root.join("linked")).unwrap();
        std::fs::write(root.join("own/solo.bin"), vec![1_u8; 300_000]).unwrap();
        std::fs::write(root.join("linked/first.bin"), vec![2_u8; 200_000]).unwrap();
        std::fs::hard_link(root.join("linked/first.bin"), root.join("own/second.bin")).unwrap();
        root
    }

    #[test]
    fn leaves_out_hard_linked_files() {
        let root = fixture("links");
        let report = scan(&root);
        let exclusive = Exclusive::measure(&report.tree, 2);
        std::fs::remove_dir_all(&root).unwrap();
        let tree = &report.tree;
        let own = tree.entry(tree.find(&root.join("own")).unwrap());
        let linked = tree.entry(tree.find(&root.join("linked")).unwrap());
        // The scan counts the linked file once, at linked/first.bin. Removing either path
        // frees nothing while the other link remains.
        assert_eq!(exclusive.bytes(linked), 0);
        assert_eq!(exclusive.bytes(own), own.allocated_bytes());
        assert_eq!(exclusive.bytes(report.root()), own.allocated_bytes());

        let mut plain = Vec::new();
        write_plain(&mut plain, &report, Selection::Depth(1), &exclusive).unwrap();
        let plain = String::from_utf8(plain).unwrap();
        assert!(
            plain.starts_with("      Size   Exclusive  Path\n"),
            "{plain}"
        );
        assert!(plain.trim_end().ends_with("[total]"), "{plain}");
        let json =
            serde_json::to_value(JsonExclusive::new(&report, Selection::Depth(1), &exclusive))
                .unwrap();
        assert_eq!(json["exclusiveBytes"], own.allocated_bytes());
        assert_eq!(json["depth"], 1);
        let linked_row = json["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["path"].as_str().unwrap().ends_with("linked"))
            .unwrap();
        assert_eq!(linked_row["exclusiveBytes"], 0);
        assert_eq!(linked_row["kind"], "directory");
        let top = serde_json::to_value(JsonExclusive::new(&report, Selection::Top(1), &exclusive))
            .unwrap();
        assert_eq!(top["topLimit"], 1);
        assert_eq!(
            top["topFiles"][0]["exclusiveBytes"],
            top["topFiles"][0]["allocatedBytes"]
        );
        assert!(top.get("entries").is_none() && top["topFiles"][0].get("kind").is_none());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn leaves_out_blocks_shared_with_clones() {
        use std::os::unix::ffi::OsStrExt;
        let root = fixture("clones");
        let source =
            std::ffi::CString::new(root.join("own/solo.bin").as_os_str().as_bytes()).unwrap();
        let clone =
            std::ffi::CString::new(root.join("own/clone.bin").as_os_str().as_bytes()).unwrap();
        // SAFETY: both paths are C strings.
        let cloned = unsafe { libc::clonefile(source.as_ptr(), clone.as_ptr(), 0) } == 0;
        let report = scan(&root);
        let exclusive = Exclusive::measure(&report.tree, 1);
        std::fs::remove_dir_all(&root).unwrap();
        if !cloned {
            return; // The temporary directory isn't on APFS.
        }
        let tree = &report.tree;
        let own = tree.entry(tree.find(&root.join("own")).unwrap());
        let solo = tree.entry(tree.find(&root.join("own/solo.bin")).unwrap());
        assert!(solo.allocated_bytes() >= 300_000);
        // Each copy shares all its blocks with the other, so neither frees them alone.
        assert_eq!(exclusive.bytes(solo), 0);
        assert!(exclusive.bytes(own) < own.allocated_bytes() - 300_000);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn maps_a_files_extents() {
        let path = std::env::temp_dir().join(format!("space16x-fiemap-{}", std::process::id()));
        std::fs::write(&path, vec![3_u8; 1_000_000]).unwrap();
        let result = linux::shared_extent_bytes(&path);
        std::fs::remove_file(&path).unwrap();
        // tmpfs doesn't map extents. Elsewhere a new file shares nothing.
        match result {
            Ok(shared) => assert_eq!(shared, 0),
            Err(error) => assert_eq!(error.raw_os_error(), Some(libc::EOPNOTSUPP)),
        }
    }

    #[test]
    fn counts_unshared_trees_in_full() {
        let mut builder =
            crate::scanner::TreeBuilder::new("/no/such/space16x".into(), EntryState::Local);
        builder.file(b"gone.bin", 4096, 10, EntryState::Local);
        let report = Report {
            tree: Arc::new(builder.finish().0),
            issues: Vec::new(),
            excluded_count: 0,
            is_preview: false,
        };
        // Files that can't be read count as unshared.
        let exclusive = Exclusive::measure(&report.tree, 4);
        assert_eq!(exclusive.bytes(report.root()), 4096);
    }
}
