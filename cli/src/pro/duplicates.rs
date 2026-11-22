//! `space16x duplicates`: files with identical contents. This is the one command that
//! reads file contents, and only because the user ran it, never during a scan.
//!
//! It scans metadata, groups files by length, and reads only files that share a length
//! with another: first their first 64 KB, then whole files whose starts match. Contents
//! are compared by std's SipHash under random keys, so a collision is about as likely as
//! 1 in 2^64 for each pair and can't be crafted.

use std::collections::HashMap;
use std::fs::File;
use std::hash::{BuildHasher, Hasher, RandomState};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use serde::Serialize;

use super::exclusive::Sharing;
use crate::report::{self, JsonPath, Metric, format_bytes, terminal_path};
use crate::scanner::{self, Entry, EntryKind, EntryState, Issue, IssueKind, NodeId, Report, Tree};
use crate::{ExcludeOptions, scan_error};

/// The bytes read first from each candidate.
const HEAD: u64 = 64 * 1024;
/// The default smallest file length compared.
pub const DEFAULT_MIN_SIZE: u64 = 1_000_000;

#[derive(Debug, PartialEq, Eq)]
pub struct DuplicatesOptions {
    pub path: PathBuf,
    pub min_size: u64,
    pub json: bool,
    pub excludes: ExcludeOptions,
    pub threads: Option<usize>,
}

/// Files with the same contents, in path order.
#[derive(Debug, PartialEq, Eq)]
pub struct Group {
    /// The file length each copy has.
    pub length: u64,
    pub files: Vec<NodeId>,
    /// The bytes of each file that moving it alone to the Trash wouldn't free, because it
    /// shares them with a clone or reflink, or has another hard link. Zero for most.
    pub shared: Vec<u64>,
}

impl Group {
    /// Whether the copy at `index` shares blocks with another file, such as an APFS
    /// clone, so that removing it alone frees only its own blocks.
    pub fn is_clone(&self, index: usize) -> bool {
        self.shared.get(index).is_some_and(|&shared| shared > 0)
    }

    /// The bytes that moving the copy at `index` alone to the Trash frees at least.
    pub fn exclusive_bytes(&self, tree: &Tree, index: usize) -> u64 {
        let allocated = tree.entry(self.files[index]).allocated_bytes();
        allocated.saturating_sub(self.shared.get(index).copied().unwrap_or(0))
    }

    /// The space that moving every copy but one to the Trash and emptying it frees at
    /// least. Each copy frees its exclusive bytes. The copy kept is the largest, or, when
    /// some copies share blocks, the clone that would free the most, since its blocks may
    /// be the ones the other clones share.
    pub fn extra_bytes(&self, tree: &Tree) -> u64 {
        let any_clone = (0..self.files.len()).any(|index| self.is_clone(index));
        let (total, kept) = (0..self.files.len()).fold((0_u64, 0_u64), |(total, kept), index| {
            let bytes = self.exclusive_bytes(tree, index);
            let keepable = !any_clone || self.is_clone(index);
            (
                total.saturating_add(bytes),
                if keepable { kept.max(bytes) } else { kept },
            )
        });
        total - kept
    }
}

/// Parses a size such as `1000000`, `500K`, `1M` or `2GB`, in decimal units.
pub fn parse_size(text: &str) -> Option<u64> {
    let upper = text.to_ascii_uppercase();
    let trimmed = upper.strip_suffix('B').unwrap_or(&upper);
    let (digits, scale) = match trimmed.char_indices().last()? {
        (index, 'K') => (&trimmed[..index], 1_000),
        (index, 'M') => (&trimmed[..index], 1_000_000),
        (index, 'G') => (&trimmed[..index], 1_000_000_000),
        (index, 'T') => (&trimmed[..index], 1_000_000_000_000),
        _ => (trimmed, 1),
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits.parse::<u64>().ok()?.checked_mul(scale)
}

pub fn run(options: DuplicatesOptions) -> io::Result<u8> {
    let excludes = options.excludes.load()?;
    let threads = options.threads.unwrap_or_else(scanner::default_threads);
    let mut report = scanner::scan(
        &options.path,
        scanner::Options {
            excludes: &excludes,
            threads,
        },
        false,
        &AtomicBool::new(false),
        |_| {},
    )
    .map_err(|error| scan_error(&options.path, error))?;
    let (groups, unread) = find(
        &report.tree,
        options.min_size,
        threads,
        &AtomicBool::new(false),
    );
    report.issues.extend(unread);
    report.issues.sort_by(|a, b| a.path.cmp(&b.path));
    let stdout = io::stdout().lock();
    if options.json {
        let result = JsonDuplicates::new(&report, &groups, options.min_size);
        report::write_json_report(
            stdout,
            &report,
            Metric::Allocated,
            excludes.sources(),
            None,
            result,
        )?;
    } else {
        write_plain(stdout, &report, &groups)?;
        crate::report_issues(&report.issues, true);
        crate::report_excluded(report.excluded_count, excludes.sources().len());
    }
    Ok(crate::status(&report))
}

/// Groups the counted files of at least `min_size` bytes in `tree` by contents, most
/// extra space first, and the files that couldn't be read. It stops early, with partial
/// results, when `cancelled` is set.
pub fn find(
    tree: &Tree,
    min_size: u64,
    threads: usize,
    cancelled: &AtomicBool,
) -> (Vec<Group>, Vec<Issue>) {
    let mut by_length: HashMap<u64, Vec<NodeId>> = HashMap::new();
    for entry in tree.root().descendants() {
        if entry.kind() == EntryKind::File
            && entry.state() == EntryState::Local
            && entry.apparent_bytes() >= min_size.max(1)
        {
            by_length
                .entry(entry.apparent_bytes())
                .or_default()
                .push(entry.id());
        }
    }
    let candidates: Vec<(u64, Vec<NodeId>)> = by_length
        .into_iter()
        .filter(|(_, files)| files.len() > 1)
        .collect();
    let keys = RandomState::new();
    let next = AtomicUsize::new(0);
    let found = Mutex::new((Vec::new(), Vec::new()));
    std::thread::scope(|scope| {
        for _ in 0..threads.clamp(1, candidates.len().max(1)) {
            scope.spawn(|| {
                while let Some((length, files)) = candidates
                    .get(next.fetch_add(1, Ordering::Relaxed))
                    .filter(|_| !cancelled.load(Ordering::Relaxed))
                {
                    let (groups, unread) = compare(tree, *length, files, &keys, cancelled);
                    let mut found = found.lock().unwrap_or_else(|error| error.into_inner());
                    found.0.extend(groups);
                    found.1.extend(unread);
                }
            });
        }
    });
    let (mut groups, unread) = found
        .into_inner()
        .unwrap_or_else(|error| error.into_inner());
    // One metadata call per copy finds the ones that share blocks.
    let sharing = Sharing::of(tree.root_path());
    for group in &mut groups {
        group
            .files
            .sort_by(|&left, &right| tree.cmp_paths(left, right));
        group.shared = group
            .files
            .iter()
            .map(|&id| {
                let entry = tree.entry(id);
                sharing
                    .shared_bytes(&entry.path())
                    .min(entry.allocated_bytes())
            })
            .collect();
    }
    rank(tree, &mut groups);
    (groups, unread)
}

/// Orders groups by their extra space, most first, then by their first file's path.
pub fn rank(tree: &Tree, groups: &mut [Group]) {
    groups.sort_by(|left, right| {
        right
            .extra_bytes(tree)
            .cmp(&left.extra_bytes(tree))
            .then_with(|| tree.cmp_paths(left.files[0], right.files[0]))
    });
}

/// A duplicate search for the explorer's Duplicates view, running on its own threads. It
/// reads files only because the user started it there. Dropping it stops the search.
pub struct Search {
    job: Arc<SearchJob>,
}

struct SearchJob {
    cancelled: AtomicBool,
    found: OnceLock<Found>,
}

/// A finished search: each group's length and copies, by path with their shared bytes, so
/// the groups outlive the tree after Move to Trash, and the number of unreadable files.
pub struct Found {
    pub groups: Vec<(u64, Vec<(PathBuf, u64)>)>,
    pub unread: usize,
}

impl Found {
    /// The groups whose copies `tree` still holds, two or more each, most extra space first.
    pub fn groups_in(&self, tree: &Tree) -> Vec<Group> {
        let mut groups: Vec<Group> = self
            .groups
            .iter()
            .filter_map(|(length, copies)| {
                let (files, shared): (Vec<NodeId>, Vec<u64>) = copies
                    .iter()
                    .filter_map(|(path, shared)| Some((tree.find(path)?, *shared)))
                    .unzip();
                (files.len() > 1).then_some(Group {
                    length: *length,
                    files,
                    shared,
                })
            })
            .collect();
        rank(tree, &mut groups);
        groups
    }
}

impl Search {
    /// Starts comparing the files of at least `min_size` bytes in `tree`.
    pub fn start(tree: &Arc<Tree>, min_size: u64, threads: usize) -> io::Result<Self> {
        let job = Arc::new(SearchJob {
            cancelled: AtomicBool::new(false),
            found: OnceLock::new(),
        });
        let shared = Arc::clone(&job);
        let searched = Arc::clone(tree);
        std::thread::Builder::new()
            .name("space16x-duplicates".into())
            .spawn(move || {
                let (groups, unread) = find(&searched, min_size, threads, &shared.cancelled);
                if shared.cancelled.load(Ordering::Relaxed) {
                    return;
                }
                let groups = groups
                    .into_iter()
                    .map(|group| {
                        let copies = group
                            .files
                            .iter()
                            .zip(&group.shared)
                            .map(|(&id, &bytes)| (searched.path(id), bytes))
                            .collect();
                        (group.length, copies)
                    })
                    .collect();
                let _ = shared.found.set(Found {
                    groups,
                    unread: unread.len(),
                });
            })?;
        Ok(Self { job })
    }

    /// The finished search.
    pub fn found(&self) -> Option<&Found> {
        self.job.found.get()
    }
}

impl Drop for Search {
    fn drop(&mut self) {
        self.job.cancelled.store(true, Ordering::Relaxed);
    }
}

/// Splits same-length files into groups with the same contents, reading their first 64 KB
/// and then, for files longer than that, the whole file.
fn compare(
    tree: &Tree,
    length: u64,
    files: &[NodeId],
    keys: &RandomState,
    cancelled: &AtomicBool,
) -> (Vec<Group>, Vec<Issue>) {
    let mut unread = Vec::new();
    let mut digest = |id: NodeId, limit: u64| -> Option<u64> {
        let path = tree.path(id);
        match hash_file(&path, limit, keys, cancelled) {
            Ok(hash) => Some(hash),
            Err(error) => {
                unread.push(Issue {
                    message: format!("Couldn't read it to compare: {error}"),
                    kind: IssueKind::of(&error),
                    path,
                });
                None
            }
        }
    };
    let mut groups = Vec::new();
    for heads in split(files, |id| digest(id, HEAD)) {
        if length <= HEAD {
            groups.push(Group {
                length,
                files: heads,
                shared: Vec::new(),
            });
            continue;
        }
        for whole in split(&heads, |id| digest(id, u64::MAX)) {
            groups.push(Group {
                length,
                files: whole,
                shared: Vec::new(),
            });
        }
    }
    (groups, unread)
}

/// Groups `files` by `key`, leaving out files without one and groups of one.
fn split(files: &[NodeId], mut key: impl FnMut(NodeId) -> Option<u64>) -> Vec<Vec<NodeId>> {
    let mut groups: HashMap<u64, Vec<NodeId>> = HashMap::new();
    for &id in files {
        if let Some(hash) = key(id) {
            groups.entry(hash).or_default().push(id);
        }
    }
    groups
        .into_values()
        .filter(|group| group.len() > 1)
        .collect()
}

/// SipHash of the first `limit` bytes of the file, and its length read.
fn hash_file(
    path: &Path,
    limit: u64,
    keys: &RandomState,
    cancelled: &AtomicBool,
) -> io::Result<u64> {
    let mut file = File::open(path)?.take(limit);
    let mut hasher = keys.build_hasher();
    let mut buffer = vec![0; 256 * 1024];
    let mut read = 0_u64;
    loop {
        if cancelled.load(Ordering::Relaxed) {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.write(&buffer[..count]);
        read += count as u64;
    }
    hasher.write_u64(read);
    Ok(hasher.finish())
}

pub fn write_plain(out: impl Write, report: &Report, groups: &[Group]) -> io::Result<()> {
    let mut out = io::BufWriter::new(out);
    writeln!(
        out,
        "{:>10}  {:>10}  {:>6}  Files",
        "Extra", "Size", "Copies"
    )?;
    let mut extra = 0_u64;
    let mut files = 0;
    for group in groups {
        extra = extra.saturating_add(group.extra_bytes(&report.tree));
        files += group.files.len();
        for (index, &id) in group.files.iter().enumerate() {
            let mut path = terminal_path(&report.tree.path(id));
            if group.is_clone(index) {
                path.push_str("  (clone)");
            }
            if index == 0 {
                writeln!(
                    out,
                    "{:>10}  {:>10}  {:>6}  {path}",
                    format_bytes(group.extra_bytes(&report.tree)),
                    format_bytes(report.tree.entry(id).allocated_bytes()),
                    group.files.len()
                )?;
            } else {
                writeln!(out, "{:>32}{path}", "")?;
            }
        }
    }
    let summary = match groups.len() {
        1 => format!("1 group of {files} files"),
        count => format!("{count} groups, {files} files"),
    };
    writeln!(out, "{:>10}  {summary}  [total]", format_bytes(extra))?;
    out.flush()
}

/// The part of a JSON report that `duplicates` adds.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonDuplicates {
    min_size: u64,
    duplicate_bytes: u64,
    groups: Vec<JsonGroup>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonGroup {
    apparent_bytes: u64,
    extra_bytes: u64,
    files: Vec<JsonFile>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonFile {
    #[serde(flatten)]
    path: JsonPath<'static>,
    allocated_bytes: u64,
    exclusive_bytes: u64,
    clone: bool,
}

impl JsonDuplicates {
    pub fn new(report: &Report, groups: &[Group], min_size: u64) -> Self {
        let groups: Vec<JsonGroup> = groups
            .iter()
            .map(|group| JsonGroup {
                apparent_bytes: group.length,
                extra_bytes: group.extra_bytes(&report.tree),
                files: group
                    .files
                    .iter()
                    .enumerate()
                    .map(|(index, &id)| {
                        let entry: Entry<'_> = report.tree.entry(id);
                        JsonFile {
                            path: JsonPath::owned(&entry.path()),
                            allocated_bytes: entry.allocated_bytes(),
                            exclusive_bytes: group.exclusive_bytes(&report.tree, index),
                            clone: group.is_clone(index),
                        }
                    })
                    .collect(),
            })
            .collect();
        Self {
            min_size,
            duplicate_bytes: groups
                .iter()
                .fold(0, |total, group| total.saturating_add(group.extra_bytes)),
            groups,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sizes_in_decimal_units() {
        assert_eq!(parse_size("0"), Some(0));
        assert_eq!(parse_size("1000000"), Some(1_000_000));
        assert_eq!(parse_size("500K"), Some(500_000));
        assert_eq!(parse_size("1m"), Some(1_000_000));
        assert_eq!(parse_size("2GB"), Some(2_000_000_000));
        assert_eq!(parse_size("3T"), Some(3_000_000_000_000));
        for invalid in ["", "M", "1.5M", "-1", "10X", "99999999999999999999", "1MM"] {
            assert_eq!(parse_size(invalid), None, "{invalid}");
        }
    }

    #[test]
    fn groups_files_with_the_same_contents() {
        let root = std::env::temp_dir().join(format!("space16x-duplicates-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("b")).unwrap();
        let big: Vec<u8> = (0..200_000_u32)
            .map(|value| (value * 7 % 251) as u8)
            .collect();
        let mut late = big.clone();
        *late.last_mut().unwrap() ^= 1;
        let small = vec![9_u8; 40_000];
        for (name, contents) in [
            ("a.bin", &big),
            ("b/copy.bin", &big),
            ("b/z.bin", &big),
            ("late.bin", &late),
            ("s1", &small),
            ("b/s2", &small),
            ("tiny1", &vec![1_u8; 10]),
            ("tiny2", &vec![1_u8; 10]),
        ] {
            std::fs::write(root.join(name), contents).unwrap();
        }
        std::fs::hard_link(root.join("s1"), root.join("s1-link")).unwrap();
        let report = scanner::scan(
            &root,
            scanner::Options {
                excludes: &scanner::Excludes::new(Vec::<String>::new()).unwrap(),
                threads: 2,
            },
            false,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        let (groups, unread) = find(&report.tree, 1_000, 3, &AtomicBool::new(false));
        std::fs::remove_dir_all(&root).unwrap();
        assert!(unread.is_empty());
        let names: Vec<Vec<String>> = groups
            .iter()
            .map(|group| {
                group
                    .files
                    .iter()
                    .map(|&id| {
                        report
                            .tree
                            .relative_path(id, NodeId::ROOT)
                            .to_string_lossy()
                            .into_owned()
                    })
                    .collect()
            })
            .collect();
        // late.bin differs only in its last byte. The hard link isn't a copy, and files
        // under the minimum aren't compared.
        assert_eq!(
            names,
            [vec!["a.bin", "b/copy.bin", "b/z.bin"], vec!["b/s2", "s1"]]
        );
        assert_eq!(groups[0].length, 200_000);
        let extra = groups[0].extra_bytes(&report.tree);
        assert!(extra >= 400_000, "{extra}");

        let mut plain = Vec::new();
        write_plain(&mut plain, &report, &groups).unwrap();
        let plain = String::from_utf8(plain).unwrap();
        let lines: Vec<&str> = plain.lines().collect();
        assert_eq!(lines[0], "     Extra        Size  Copies  Files");
        assert!(
            lines[1].contains("       3  /") && lines[1].ends_with("a.bin"),
            "{plain}"
        );
        assert!(lines[2].trim_start().ends_with("b/copy.bin"), "{plain}");
        assert!(
            plain.trim_end().ends_with("2 groups, 5 files  [total]"),
            "{plain}"
        );
        let json = serde_json::to_value(JsonDuplicates::new(&report, &groups, 1_000)).unwrap();
        assert_eq!(json["minSize"], 1_000);
        assert_eq!(json["groups"][0]["apparentBytes"], 200_000);
        assert_eq!(json["groups"][0]["files"].as_array().unwrap().len(), 3);
        assert_eq!(
            json["duplicateBytes"],
            groups[0].extra_bytes(&report.tree) + groups[1].extra_bytes(&report.tree)
        );
        assert_eq!(json["groups"][0]["files"][0]["clone"], false);
        assert_eq!(
            json["groups"][0]["files"][0]["exclusiveBytes"],
            json["groups"][0]["files"][0]["allocatedBytes"]
        );
        assert!(
            find(&report.tree, 300_000, 1, &AtomicBool::new(false))
                .0
                .is_empty()
        );
        // A cancelled search reads nothing.
        assert!(
            find(&report.tree, 1_000, 2, &AtomicBool::new(true))
                .0
                .is_empty()
        );
    }

    /// The Mac app's `testCountsOnlyWhatRemovingClonesFrees` builds the same folder: a.bin,
    /// a separate copy b.bin, and c.bin, an APFS clone of a.bin.
    #[cfg(target_os = "macos")]
    #[test]
    fn counts_only_what_removing_clones_frees() {
        use std::os::unix::ffi::OsStrExt;
        let root =
            std::env::temp_dir().join(format!("space16x-duplicate-clones-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let contents: Vec<u8> = (0..300_000_u32)
            .map(|value| (value * 13 % 251) as u8)
            .collect();
        std::fs::write(root.join("a.bin"), &contents).unwrap();
        std::fs::write(root.join("b.bin"), &contents).unwrap();
        let source = std::ffi::CString::new(root.join("a.bin").as_os_str().as_bytes()).unwrap();
        let clone = std::ffi::CString::new(root.join("c.bin").as_os_str().as_bytes()).unwrap();
        // SAFETY: both paths are C strings.
        let cloned = unsafe { libc::clonefile(source.as_ptr(), clone.as_ptr(), 0) } == 0;
        let report = scanner::scan(
            &root,
            scanner::Options {
                excludes: &scanner::Excludes::new(Vec::<String>::new()).unwrap(),
                threads: 2,
            },
            false,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        let (groups, _) = find(&report.tree, 1_000, 2, &AtomicBool::new(false));
        std::fs::remove_dir_all(&root).unwrap();
        if !cloned {
            return; // The temporary directory isn't on APFS.
        }
        assert_eq!(groups.len(), 1);
        let group = &groups[0];
        assert_eq!(group.files.len(), 3);
        let clones: Vec<bool> = (0..3).map(|index| group.is_clone(index)).collect();
        assert_eq!(clones, [true, false, true]);
        assert_eq!(group.exclusive_bytes(&report.tree, 0), 0);
        // Removing b.bin frees its blocks. Removing a clone frees nothing while the other
        // clone keeps their blocks.
        let copy = report.tree.entry(group.files[1]).allocated_bytes();
        assert!(copy >= 300_000);
        assert_eq!(group.extra_bytes(&report.tree), copy);
        let mut plain = Vec::new();
        write_plain(&mut plain, &report, &groups).unwrap();
        let plain = String::from_utf8(plain).unwrap();
        assert_eq!(plain.matches("  (clone)").count(), 2, "{plain}");
        let json = serde_json::to_value(JsonDuplicates::new(&report, &groups, 1_000)).unwrap();
        assert_eq!(json["duplicateBytes"], copy);
        assert_eq!(json["groups"][0]["files"][2]["clone"], true);
        assert_eq!(json["groups"][0]["files"][2]["exclusiveBytes"], 0);
    }
}
