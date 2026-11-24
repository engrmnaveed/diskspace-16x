use super::*;
use std::fs::{self, File};
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::sync::atomic::AtomicU64;
use std::time::Duration;

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

/// Threads for most tests, so they exercise the parallel walk.
const THREADS: usize = 4;

fn options(excludes: &Excludes) -> Options<'_> {
    Options {
        excludes,
        threads: THREADS,
    }
}

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> io::Result<Self> {
        let folder = std::env::temp_dir().join(format!(
            "diskuse16x-rust-test-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&folder)?;
        Ok(Self(folder))
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn join(&self, path: impl AsRef<Path>) -> PathBuf {
        self.path().join(path)
    }

    fn report(&self) -> io::Result<Report> {
        scan(
            self.path(),
            options(&Excludes::default()),
            false,
            &AtomicBool::new(false),
            |_| {},
        )
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn child<'a>(directory: Entry<'a>, name: &str) -> Entry<'a> {
    directory
        .children()
        .find(|entry| entry.name() == name)
        .unwrap_or_else(|| panic!("missing child {name}"))
}

#[test]
fn counts_metadata_and_hard_links_in_bytewise_order() -> io::Result<()> {
    let fixture = Fixture::new()?;
    fs::create_dir(fixture.join("A"))?;
    fs::create_dir(fixture.join("z"))?;
    fs::write(fixture.join("z/original"), [0xAB; 4_097])?;
    fs::hard_link(fixture.join("z/original"), fixture.join("A/alias"))?;
    fs::write(fixture.join(".hidden"), [0xCD; 13])?;
    fs::set_permissions(fixture.join(".hidden"), fs::Permissions::from_mode(0o0))?;
    symlink(fixture.path(), fixture.join("loop"))?;
    symlink(fixture.join("z/original"), fixture.join("file_link"))?;
    let file_bytes = fs::metadata(fixture.join("z/original"))?.blocks() * 512;
    let hidden_bytes = fs::metadata(fixture.join(".hidden"))?.blocks() * 512;

    let report = fixture.report()?;

    assert!(report.issues.is_empty());
    assert_eq!(report.root().children().count(), 3);
    assert_eq!(report.root().allocated_bytes(), file_bytes + hidden_bytes);
    let counted = child(child(report.root(), "A"), "alias");
    assert_eq!(counted.state(), EntryState::Local);
    assert_eq!(counted.allocated_bytes(), file_bytes);
    let uncounted = child(child(report.root(), "z"), "original");
    assert_eq!(uncounted.state(), EntryState::HardLinkAlias);
    assert_eq!(uncounted.allocated_bytes(), 0);
    // Apparent sizes and item counts follow the same hard-link rule.
    assert_eq!(counted.apparent_bytes(), 4_097);
    assert_eq!(uncounted.apparent_bytes(), 0);
    assert_eq!(report.root().apparent_bytes(), 4_097 + 13);
    // A, A/alias, z and .hidden. The second link and the symlinks aren't items.
    assert_eq!(report.root().item_count(), 4);
    assert_eq!(child(report.root(), "A").item_count(), 1);
    assert_eq!(child(report.root(), "z").item_count(), 0);
    Ok(())
}

#[test]
fn counts_nested_items_and_apparent_bytes() -> io::Result<()> {
    let fixture = Fixture::new()?;
    fs::create_dir_all(fixture.join("one/two/three"))?;
    fs::write(fixture.join("one/a"), [1; 10])?;
    fs::write(fixture.join("one/two/b"), [2; 20])?;
    fs::write(fixture.join("one/two/three/c"), [3; 30])?;
    fs::create_dir(fixture.join("empty"))?;

    let report = fixture.report()?;

    let one = child(report.root(), "one");
    assert_eq!(one.item_count(), 5);
    assert_eq!(one.apparent_bytes(), 60);
    assert_eq!(child(one, "two").item_count(), 3);
    assert_eq!(child(report.root(), "empty").item_count(), 0);
    assert_eq!(report.root().item_count(), 7);
    assert_eq!(report.root().apparent_bytes(), 60);
    Ok(())
}

#[test]
fn skips_excluded_entries_without_reading_them() -> io::Result<()> {
    let fixture = Fixture::new()?;
    fs::create_dir_all(fixture.join("app/node_modules/pkg"))?;
    fs::write(fixture.join("app/node_modules/pkg/index.js"), [1; 100])?;
    fs::write(fixture.join("app/main.rs"), [2; 10])?;
    fs::write(fixture.join("app/debug.log"), [3; 50])?;
    fs::create_dir(fixture.join("cache"))?;
    fs::write(fixture.join("cache/blob"), [4; 70])?;
    fs::write(fixture.join("app/cache"), [5; 7])?;
    // An excluded folder that can't be read reports no issue, because it isn't opened.
    fs::create_dir(fixture.join("private"))?;
    fs::set_permissions(fixture.join("private"), fs::Permissions::from_mode(0o0))?;
    let excludes = Excludes::new(["node_modules", "*.log", "/cache/", "/private"])
        .map_err(io::Error::other)?;

    let report = scan(
        fixture.path(),
        options(&excludes),
        false,
        &AtomicBool::new(false),
        |_| {},
    );
    fs::set_permissions(fixture.join("private"), fs::Permissions::from_mode(0o700))?;
    let report = report?;

    assert!(report.issues.is_empty(), "{:?}", report.issues);
    assert_eq!(report.excluded_count, 4);
    // app, app/main.rs and app/cache, a file that the folder-only /cache/ doesn't match.
    assert_eq!(report.root().item_count(), 3);
    assert_eq!(report.root().apparent_bytes(), 17);
    let app = child(report.root(), "app");
    assert_eq!(app.children().count(), 2);
    Ok(())
}

#[test]
fn excludes_match_below_the_filesystem_root() {
    assert_eq!(relative_bytes(1, Path::new("/tmp/x")), b"tmp/x");
    assert_eq!(relative_bytes(4, Path::new("/tmp/x/y")), b"x/y");
}

#[test]
fn does_not_count_sparse_file_length() -> io::Result<()> {
    let fixture = Fixture::new()?;
    let file = File::create(fixture.join("sparse.img"))?;
    let logical = 64 * 1024 * 1024;
    file.set_len(logical)?;
    let metadata = file.metadata()?;

    let report = fixture.report()?;

    assert_eq!(report.root().allocated_bytes(), metadata.blocks() * 512);
    assert!(report.root().allocated_bytes() < logical);
    assert_eq!(report.root().apparent_bytes(), logical);
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn ordinary_directory_with_startup_volume_layout_counts_both_trees() -> io::Result<()> {
    let fixture = Fixture::new()?;
    fs::create_dir(fixture.join("Users"))?;
    fs::create_dir_all(fixture.join("System/Volumes/Data/Users"))?;
    let visible = fixture.join("Users/payload");
    let backing = fixture.join("System/Volumes/Data/Users/payload");
    fs::write(&visible, [0x21; 8_192])?;
    fs::write(&backing, [0x21; 8_192])?;

    let report = scan(
        fixture.path(),
        options(&Excludes::default()),
        true,
        &AtomicBool::new(false),
        |_| {},
    )?;

    assert!(report.issues.is_empty());
    assert_eq!(
        report.root().allocated_bytes(),
        (fs::metadata(visible)?.blocks() + fs::metadata(backing)?.blocks()) * 512
    );
    Ok(())
}

#[test]
fn rejects_non_directory_roots_and_root_symlinks_with_trailing_slash() -> io::Result<()> {
    let fixture = Fixture::new()?;
    fs::create_dir(fixture.join("local"))?;
    fs::write(fixture.join("file"), "text")?;
    symlink(fixture.join("local"), fixture.join("link"))?;
    let cancelled = AtomicBool::new(false);
    let link = fixture.join("link/");
    for root in [fixture.join("file"), fixture.join("link"), link] {
        let error = scan(
            &root,
            options(&Excludes::default()),
            false,
            &cancelled,
            |_| {},
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }
    let report = scan(
        &fixture.join("local/./"),
        options(&Excludes::default()),
        false,
        &cancelled,
        |_| {},
    )?;
    assert_eq!(report.tree.root_path(), fixture.join("local"));
    Ok(())
}

#[test]
fn reports_unreadable_descendants_and_fails_for_an_unreadable_root() -> io::Result<()> {
    // SAFETY: geteuid takes no arguments and cannot modify process state.
    if unsafe { libc::geteuid() } == 0 {
        return Ok(());
    }
    let fixture = Fixture::new()?;
    let private = fixture.join("private");
    fs::create_dir(&private)?;
    fs::write(private.join("hidden"), [0x12; 6_000])?;
    fs::write(fixture.join("visible"), [0x21; 5_300])?;
    fs::set_permissions(&private, fs::Permissions::from_mode(0o0))?;
    let report = fixture.report();
    let failed = scan(
        &private,
        options(&Excludes::default()),
        false,
        &AtomicBool::new(false),
        |_| {},
    );
    fs::set_permissions(&private, fs::Permissions::from_mode(0o700))?;

    let report = report?;
    assert_eq!(report.issues.len(), 1);
    assert_eq!(report.issues[0].path, private);
    // A permission refusal is an expected, protected path rather than a scan error.
    assert_eq!(report.issues[0].kind, IssueKind::PermissionDenied);
    assert_eq!(report.root().state(), EntryState::Protected);
    assert_eq!(
        child(report.root(), "private").state(),
        EntryState::Protected
    );
    assert_eq!(
        report.root().allocated_bytes(),
        fs::metadata(fixture.join("visible"))?.blocks() * 512
    );
    assert_eq!(failed.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn retains_non_utf8_names_and_counts_their_bytes() -> io::Result<()> {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let fixture = Fixture::new()?;
    let name = OsString::from_vec(vec![0xff]);
    let file = fixture.join(name);
    fs::write(&file, [0x21; 1_111])?;
    let report = fixture.report()?;

    assert!(report.issues.is_empty());
    assert_eq!(report.root().children().next().unwrap().path(), file);
    assert_eq!(
        report.root().allocated_bytes(),
        fs::metadata(file)?.blocks() * 512
    );
    Ok(())
}

#[test]
fn previews_leave_deep_and_hard_link_bytes_unassigned_until_dfs() -> io::Result<()> {
    let fixture = Fixture::new()?;
    fs::create_dir_all(fixture.join("A/deeper"))?;
    // A second pending folder keeps the walk going after A/deeper finishes.
    fs::create_dir_all(fixture.join("B/more"))?;
    fs::write(fixture.join("A/deeper/payload"), [0x40; 8_000])?;
    fs::hard_link(fixture.join("A/deeper/payload"), fixture.join("z-link"))?;
    let mut snapshots = Vec::new();
    let mut progress = Vec::new();

    // One thread walks in a fixed order, so the update after A/deeper is predictable.
    let final_report = scan(
        fixture.path(),
        Options {
            excludes: &Excludes::default(),
            threads: 1,
        },
        true,
        &AtomicBool::new(false),
        |event| {
            match event {
                Event::Snapshot(report) => {
                    if snapshots.is_empty() {
                        // Let the publication interval elapse before the first branch completes.
                        std::thread::sleep(Duration::from_millis(420));
                    }
                    snapshots.push(report);
                }
                Event::Progress { visited, path } => progress.push((visited, path)),
            }
        },
    )?;

    let initial = &snapshots[0];
    assert!(initial.is_preview);
    assert_eq!(initial.root().state(), EntryState::Pending);
    assert_eq!(
        child(child(initial.root(), "A"), "deeper").state(),
        EntryState::Pending
    );
    assert!(
        child(child(initial.root(), "A"), "deeper")
            .children()
            .next()
            .is_none()
    );
    assert_eq!(child(initial.root(), "z-link").state(), EntryState::Pending);
    assert_eq!(initial.root().allocated_bytes(), 0);

    let update = &snapshots[1];
    assert!(update.is_preview);
    let deeper = child(child(update.root(), "A"), "deeper");
    assert_eq!(deeper.state(), EntryState::Local);
    assert_eq!(child(deeper, "payload").kind(), EntryKind::File);
    assert_eq!(
        deeper.allocated_bytes(),
        final_report.root().allocated_bytes()
    );
    assert_eq!(
        child(child(update.root(), "B"), "more").state(),
        EntryState::Pending
    );
    // The earlier link in A/deeper is known, so the later one is already an alias.
    assert_eq!(
        child(update.root(), "z-link").state(),
        EntryState::HardLinkAlias
    );

    assert!(!snapshots.last().unwrap().is_preview);
    assert!(Arc::ptr_eq(
        &snapshots.last().unwrap().tree,
        &final_report.tree
    ));
    assert_eq!(
        child(final_report.root(), "z-link").state(),
        EntryState::HardLinkAlias
    );
    assert_eq!(progress.first().unwrap().1, fixture.path());
    assert_eq!(progress.last().unwrap().1, fixture.path());
    assert!(progress.windows(2).all(|pair| pair[0].0 <= pair[1].0));
    Ok(())
}

#[test]
fn cancellation_preserves_the_emitted_preview_without_a_final_snapshot() -> io::Result<()> {
    let fixture = Fixture::new()?;
    fs::create_dir_all(fixture.join("A/deeper"))?;
    let cancelled = AtomicBool::new(false);
    let mut snapshots = Vec::new();
    let result = scan(
        fixture.path(),
        options(&Excludes::default()),
        true,
        &cancelled,
        |event| {
            if let Event::Snapshot(report) = event {
                snapshots.push(report);
                cancelled.store(true, Ordering::Relaxed);
            }
        },
    );

    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::Interrupted);
    assert_eq!(snapshots.len(), 1);
    assert!(snapshots[0].is_preview);
    Ok(())
}

#[test]
fn keeps_the_most_serious_state_when_merging_skipped_descendants() {
    use EntryState::{Incomplete, Local, Pending, Protected};
    assert_eq!(Local.degrade(Protected), Protected);
    assert_eq!(Local.degrade(Incomplete), Incomplete);
    assert_eq!(Protected.degrade(Incomplete), Incomplete);
    assert_eq!(Incomplete.degrade(Protected), Incomplete);
    assert_eq!(Protected.degrade(Local), Protected);
    assert_eq!(Pending.degrade(Incomplete), Pending);

    let mut calm = TreeBuilder::new(PathBuf::from("/a"), Local);
    calm.folder(b"protected", Protected);
    assert_eq!(calm.finish().0.root().state(), Protected);
    let mut mixed = TreeBuilder::new(PathBuf::from("/a"), Local);
    mixed.folder(b"failed", Incomplete);
    mixed.folder(b"protected", Protected);
    assert_eq!(mixed.finish().0.root().state(), Incomplete);
}

#[test]
fn classifies_access_refusals_as_protected() {
    let kind = |code| IssueKind::of(&io::Error::from_raw_os_error(code));
    assert_eq!(kind(libc::EACCES), IssueKind::PermissionDenied);
    assert_eq!(kind(libc::EPERM), IssueKind::NotPermitted);
    assert_eq!(kind(libc::EIO), IssueKind::Error);
    assert_eq!(
        IssueKind::of(&io::Error::from(io::ErrorKind::TimedOut)),
        IssueKind::Error
    );
    assert!(IssueKind::PermissionDenied.is_protected());
    assert!(IssueKind::NotPermitted.is_protected());
    assert!(!IssueKind::Error.is_protected());
}

fn rescan_folder(root: &Path, folder: &Path, excludes: &Excludes) -> io::Result<Report> {
    rescan(
        root,
        folder,
        options(excludes),
        &AtomicBool::new(false),
        |_| {},
    )
}

fn assert_same_tree(left: Entry<'_>, right: Entry<'_>) {
    let path = left.path();
    assert_eq!(path, right.path());
    assert_eq!(left.allocated_bytes(), right.allocated_bytes(), "{path:?}");
    assert_eq!(left.apparent_bytes(), right.apparent_bytes(), "{path:?}");
    assert_eq!(left.item_count(), right.item_count(), "{path:?}");
    assert_eq!(left.state(), right.state(), "{path:?}");
    assert_eq!(left.kind(), right.kind(), "{path:?}");
    assert_eq!(
        left.children().count(),
        right.children().count(),
        "{path:?}"
    );
    for (left, right) in left.children().zip(right.children()) {
        assert_same_tree(left, right);
    }
}

#[test]
fn rescanning_a_folder_matches_a_full_scan() -> io::Result<()> {
    let fixture = Fixture::new()?;
    fs::create_dir_all(fixture.join("outer/inner"))?;
    fs::write(fixture.join("outer/inner/old.bin"), [1; 9_000])?;
    fs::write(fixture.join("outer/keep.bin"), [2; 3_000])?;
    fs::write(fixture.join("top.bin"), [3; 5_000])?;
    let before = fixture.report()?;

    fs::remove_file(fixture.join("outer/inner/old.bin"))?;
    fs::create_dir(fixture.join("outer/inner/new"))?;
    fs::write(fixture.join("outer/inner/new/a.bin"), [4; 20_000])?;
    fs::write(fixture.join("outer/inner/b.bin"), [5; 700])?;
    let inner = fixture.join("outer/inner");
    let subtree = rescan_folder(fixture.path(), &inner, &Excludes::default())?;
    let merged = before
        .replacing(subtree)
        .expect("the folder is in the report");

    assert_same_tree(merged.root(), fixture.report()?.root());
    assert!(merged.issues.is_empty());
    // The whole scope can be rescanned too.
    let whole = rescan_folder(fixture.path(), fixture.path(), &Excludes::default())?;
    assert_same_tree(before.replacing(whole).unwrap().root(), merged.root());
    Ok(())
}

#[test]
fn rescanning_keeps_exclude_patterns_relative_to_the_scan_root() -> io::Result<()> {
    let fixture = Fixture::new()?;
    fs::create_dir_all(fixture.join("app/build"))?;
    fs::create_dir_all(fixture.join("app/src/build"))?;
    fs::write(fixture.join("app/build/out.bin"), [1; 8_000])?;
    fs::write(fixture.join("app/src/build/keep.bin"), [2; 4_000])?;
    let excludes = Excludes::new(["/app/build"]).expect("valid pattern");
    let full = scan(
        fixture.path(),
        options(&excludes),
        false,
        &AtomicBool::new(false),
        |_| {},
    )?;

    let app = fixture.join("app");
    let subtree = rescan_folder(fixture.path(), &app, &excludes)?;
    assert_eq!(subtree.excluded_count, 1);
    let merged = full.replacing(subtree).unwrap();
    assert_same_tree(merged.root(), full.root());
    assert_eq!(merged.excluded_count, 1);
    assert!(rescan_folder(&app, fixture.path(), &excludes).is_err());
    Ok(())
}

#[test]
fn rescanning_recomputes_states_above_the_folder() -> io::Result<()> {
    // SAFETY: geteuid takes no arguments and cannot modify process state.
    if unsafe { libc::geteuid() } == 0 {
        return Ok(());
    }
    let fixture = Fixture::new()?;
    let private = fixture.join("sub/private");
    let other = fixture.join("other");
    fs::create_dir_all(&private)?;
    fs::create_dir(&other)?;
    fs::write(private.join("hidden"), [7; 6_000])?;
    fs::set_permissions(&private, fs::Permissions::from_mode(0o0))?;
    fs::set_permissions(&other, fs::Permissions::from_mode(0o0))?;
    let before = fixture.report();
    fs::set_permissions(&private, fs::Permissions::from_mode(0o700))?;
    let subtree = rescan_folder(fixture.path(), &fixture.join("sub"), &Excludes::default());
    fs::set_permissions(&other, fs::Permissions::from_mode(0o700))?;

    let before = before?;
    assert_eq!(before.issues.len(), 2);
    assert_eq!(child(before.root(), "sub").state(), EntryState::Protected);
    let merged = before.replacing(subtree?).unwrap();
    let sub = child(merged.root(), "sub");
    assert_eq!(sub.state(), EntryState::Local);
    assert_eq!(
        sub.allocated_bytes(),
        fs::metadata(private.join("hidden"))?.blocks() * 512
    );
    assert_eq!(sub.item_count(), 2);
    // The unreadable sibling still keeps the scope protected.
    assert_eq!(merged.root().state(), EntryState::Protected);
    assert_eq!(merged.issues.len(), 1);
    assert_eq!(merged.issues[0].path, other);
    Ok(())
}

#[test]
fn replacing_needs_a_folder_in_the_report() -> io::Result<()> {
    let fixture = Fixture::new()?;
    fs::create_dir(fixture.join("folder"))?;
    fs::write(fixture.join("file.bin"), [1; 100])?;
    let report = fixture.report()?;
    let folder = rescan_folder(
        fixture.path(),
        &fixture.join("folder"),
        &Excludes::default(),
    )?;
    let elsewhere = |path: PathBuf| Report {
        tree: Arc::new(TreeBuilder::new(path, EntryState::Local).finish().0),
        ..folder.clone()
    };
    assert!(
        report
            .replacing(elsewhere(fixture.join("gone/folder")))
            .is_none()
    );
    assert!(
        report
            .replacing(elsewhere(fixture.join("file.bin")))
            .is_none()
    );
    Ok(())
}

/// A tree whose folders are read by different threads, with hard links between them,
/// an unreadable folder, a symlink and an excluded file.
fn busy_fixture() -> io::Result<Fixture> {
    let fixture = Fixture::new()?;
    for group in 0..6 {
        for folder in 0..12 {
            let path = fixture.join(format!("g{group}/f{folder:02}/inner"));
            fs::create_dir_all(&path)?;
            for file in 0..5 {
                let bytes = vec![group as u8; 1 + 977 * (folder * 5 + file)];
                fs::write(path.join(format!("{file}.bin")), bytes)?;
            }
            fs::write(path.join("skip.log"), [9; 50])?;
        }
    }
    // Each file below has links in several groups. The first path in depth-first order
    // owns it: g0 for the first, g1 for the second, and the top-level file for the last.
    fs::write(fixture.join("g5/f11/inner/shared.bin"), [7; 20_000])?;
    fs::hard_link(
        fixture.join("g5/f11/inner/shared.bin"),
        fixture.join("g0/f03/inner/z.bin"),
    )?;
    fs::hard_link(
        fixture.join("g5/f11/inner/shared.bin"),
        fixture.join("g3/f00/link.bin"),
    )?;
    fs::write(fixture.join("g4/f01/inner/second.bin"), [8; 30_000])?;
    fs::hard_link(
        fixture.join("g4/f01/inner/second.bin"),
        fixture.join("g1/f07/inner/a.bin"),
    )?;
    fs::write(fixture.join("top.bin"), [6; 10_000])?;
    fs::hard_link(
        fixture.join("top.bin"),
        fixture.join("g2/f05/inner/late.bin"),
    )?;
    symlink(fixture.join("g0"), fixture.join("g1/loop"))?;
    Ok(fixture)
}

#[test]
fn parallel_walks_match_a_single_thread_on_every_run() -> io::Result<()> {
    let fixture = busy_fixture()?;
    let excludes = Excludes::new(["*.log"]).map_err(io::Error::other)?;
    let scan_with = |threads| {
        scan(
            fixture.path(),
            Options {
                excludes: &excludes,
                threads,
            },
            false,
            &AtomicBool::new(false),
            |_| {},
        )
    };
    let expected = scan_with(1)?;
    assert_eq!(expected.excluded_count, 72);
    let owner = |report: &Report, path: &str| {
        let id = report.tree.find(&fixture.join(path)).unwrap();
        report.tree.entry(id).state()
    };
    for (path, state) in [
        ("g0/f03/inner/z.bin", EntryState::Local),
        ("g3/f00/link.bin", EntryState::HardLinkAlias),
        ("g5/f11/inner/shared.bin", EntryState::HardLinkAlias),
        ("g1/f07/inner/a.bin", EntryState::Local),
        ("g4/f01/inner/second.bin", EntryState::HardLinkAlias),
        ("g2/f05/inner/late.bin", EntryState::Local),
        ("top.bin", EntryState::HardLinkAlias),
    ] {
        assert_eq!(owner(&expected, path), state, "{path}");
    }
    for threads in [2, 3, 8] {
        for _ in 0..4 {
            let report = scan_with(threads)?;
            assert_same_tree(report.root(), expected.root());
            assert_eq!(report.excluded_count, expected.excluded_count);
            let issues = |report: &Report| {
                report
                    .issues
                    .iter()
                    .map(|issue| (issue.path.clone(), issue.message.clone()))
                    .collect::<Vec<_>>()
            };
            assert_eq!(issues(&report), issues(&expected));
        }
    }
    Ok(())
}

#[test]
fn parallel_walks_report_unreadable_folders_in_path_order() -> io::Result<()> {
    // SAFETY: geteuid takes no arguments and cannot modify process state.
    if unsafe { libc::geteuid() } == 0 {
        return Ok(());
    }
    let fixture = busy_fixture()?;
    let locked = [fixture.join("g4/f09"), fixture.join("g1/f02/inner")];
    for path in &locked {
        fs::set_permissions(path, fs::Permissions::from_mode(0o0))?;
    }
    let reports: Vec<_> = [1, 8]
        .into_iter()
        .map(|threads| {
            scan(
                fixture.path(),
                Options {
                    excludes: &Excludes::default(),
                    threads,
                },
                false,
                &AtomicBool::new(false),
                |_| {},
            )
        })
        .collect();
    for path in &locked {
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    let reports = reports.into_iter().collect::<io::Result<Vec<_>>>()?;
    for report in &reports {
        let paths: Vec<_> = report
            .issues
            .iter()
            .map(|issue| issue.path.clone())
            .collect();
        assert_eq!(paths, [locked[1].clone(), locked[0].clone()]);
        assert_eq!(report.root().state(), EntryState::Protected);
    }
    assert_same_tree(reports[0].root(), reports[1].root());
    Ok(())
}

#[test]
fn parallel_previews_show_two_levels_first_and_stop_when_cancelled() -> io::Result<()> {
    let fixture = busy_fixture()?;
    let cancelled = AtomicBool::new(false);
    let mut snapshots = Vec::new();
    let result = scan(
        fixture.path(),
        options(&Excludes::default()),
        true,
        &cancelled,
        |event| {
            if let Event::Snapshot(report) = event {
                snapshots.push(report);
                cancelled.store(true, Ordering::Relaxed);
            }
        },
    );
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::Interrupted);
    assert_eq!(snapshots.len(), 1);
    let initial = &snapshots[0];
    assert!(initial.is_preview);
    assert_eq!(initial.root().state(), EntryState::Pending);
    let group = child(initial.root(), "g3");
    assert_eq!(group.children().count(), 12);
    assert!(
        group
            .children()
            .all(|folder| folder.state() == EntryState::Pending)
    );
    // top.bin has a link in a pending folder that comes first, so it waits too.
    assert_eq!(
        child(initial.root(), "top.bin").state(),
        EntryState::Pending
    );

    let complete = scan(
        fixture.path(),
        options(&Excludes::default()),
        true,
        &AtomicBool::new(false),
        |_| {},
    )?;
    assert!(!complete.is_preview);
    assert_eq!(complete.root().state(), EntryState::Local);
    Ok(())
}

/// Builds the shared modification-time fixture, which the Swift tests read too.
#[cfg(feature = "pro")]
fn modified_fixture() -> io::Result<(Fixture, serde_json::Value)> {
    use std::time::UNIX_EPOCH;

    let table: serde_json::Value =
        serde_json::from_str(include_str!("../../../fixtures/modified-times.json"))
            .expect("the fixture is valid JSON");
    let fixture = Fixture::new()?;
    for folder in table["folders"].as_array().expect("folders") {
        fs::create_dir_all(fixture.join(folder.as_str().expect("a folder path")))?;
    }
    for file in table["files"].as_array().expect("files") {
        let path = fixture.join(file["path"].as_str().expect("a file path"));
        fs::create_dir_all(path.parent().expect("files are below the root"))?;
        if let Some(target) = file["hardLinkTo"].as_str() {
            fs::hard_link(fixture.join(target), &path)?;
            continue;
        }
        let seconds = file["modified"].as_i64().expect("a time");
        let time = if seconds >= 0 {
            UNIX_EPOCH + Duration::from_secs(seconds as u64)
        } else {
            UNIX_EPOCH - Duration::from_secs(seconds.unsigned_abs())
        };
        File::create(&path)?.set_modified(time)?;
    }
    Ok((fixture, table))
}

#[cfg(feature = "pro")]
#[test]
fn records_each_entrys_newest_modification_time() -> io::Result<()> {
    let (fixture, table) = modified_fixture()?;
    let uid = fs::metadata(fixture.path())?.uid();
    for threads in [1, THREADS] {
        let report = scan(
            fixture.path(),
            Options {
                excludes: &Excludes::default(),
                threads,
            },
            false,
            &AtomicBool::new(false),
            |_| {},
        )?;
        let tree = &report.tree;
        for (path, expected) in table["newest"].as_object().expect("newest") {
            let id = tree
                .find(&fixture.join(path))
                .expect("the path was scanned");
            let details = tree.entry(id).details();
            let expected = expected.as_u64().map(|seconds| seconds as u32);
            assert_eq!(
                details.modified(),
                expected,
                "{path} with {threads} threads"
            );
            assert_eq!(details.owner(), uid, "{path}");
        }
    }
    Ok(())
}
