use super::*;
use std::fs::{self, File};
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::sync::atomic::AtomicU64;

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> io::Result<Self> {
        let folder = std::env::temp_dir().join(format!(
            "space16x-rust-test-{}-{}",
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
        scan(self.path(), false, &AtomicBool::new(false), |_| {})
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn child<'a>(directory: &'a Entry, name: &str) -> &'a Entry {
    directory
        .children
        .iter()
        .find(|entry| entry.path.file_name().is_some_and(|entry| entry == name))
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
    assert_eq!(report.root.children.len(), 3);
    assert_eq!(report.root.allocated_bytes, file_bytes + hidden_bytes);
    let counted = child(child(&report.root, "A"), "alias");
    assert_eq!(counted.state, EntryState::Local);
    assert_eq!(counted.allocated_bytes, file_bytes);
    let uncounted = child(child(&report.root, "z"), "original");
    assert_eq!(uncounted.state, EntryState::HardLinkAlias);
    assert_eq!(uncounted.allocated_bytes, 0);
    Ok(())
}

#[test]
fn does_not_count_sparse_file_length() -> io::Result<()> {
    let fixture = Fixture::new()?;
    let file = File::create(fixture.join("sparse.img"))?;
    let logical = 64 * 1024 * 1024;
    file.set_len(logical)?;
    let metadata = file.metadata()?;

    let report = fixture.report()?;

    assert_eq!(report.root.allocated_bytes, metadata.blocks() * 512);
    assert!(report.root.allocated_bytes < logical);
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
        let error = scan(&root, false, &cancelled, |_| {}).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }
    let report = scan(&fixture.join("local/./"), false, &cancelled, |_| {})?;
    assert_eq!(report.root.path, fixture.join("local"));
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
    let failed = scan(&private, false, &AtomicBool::new(false), |_| {});
    fs::set_permissions(&private, fs::Permissions::from_mode(0o700))?;

    let report = report?;
    assert_eq!(report.issues.len(), 1);
    assert_eq!(report.issues[0].path, private);
    assert_eq!(report.root.state, EntryState::Incomplete);
    assert_eq!(child(&report.root, "private").state, EntryState::Incomplete);
    assert_eq!(
        report.root.allocated_bytes,
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
    assert_eq!(report.root.children[0].path, file);
    assert_eq!(
        report.root.allocated_bytes,
        fs::metadata(file)?.blocks() * 512
    );
    Ok(())
}

#[test]
fn previews_leave_deep_and_hard_link_bytes_unassigned_until_dfs() -> io::Result<()> {
    let fixture = Fixture::new()?;
    fs::create_dir_all(fixture.join("A/deeper"))?;
    fs::write(fixture.join("A/deeper/payload"), [0x40; 8_000])?;
    fs::hard_link(fixture.join("A/deeper/payload"), fixture.join("z-link"))?;
    let mut snapshots = Vec::new();
    let mut progress = Vec::new();

    let final_report = scan(fixture.path(), true, &AtomicBool::new(false), |event| {
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
    })?;

    let initial = &snapshots[0];
    assert!(initial.is_preview);
    assert_eq!(initial.root.state, EntryState::Pending);
    assert_eq!(
        child(child(&initial.root, "A"), "deeper").state,
        EntryState::Pending
    );
    assert!(
        child(child(&initial.root, "A"), "deeper")
            .children
            .is_empty()
    );
    assert_eq!(child(&initial.root, "z-link").state, EntryState::Pending);
    assert_eq!(initial.root.allocated_bytes, 0);

    let update = &snapshots[1];
    assert!(update.is_preview);
    let deeper = child(child(&update.root, "A"), "deeper");
    assert_eq!(deeper.state, EntryState::Local);
    assert_eq!(child(deeper, "payload").kind, EntryKind::File);
    assert_eq!(deeper.allocated_bytes, final_report.root.allocated_bytes);

    assert!(!snapshots.last().unwrap().is_preview);
    assert!(Arc::ptr_eq(
        &snapshots.last().unwrap().root,
        &final_report.root
    ));
    assert_eq!(
        child(&final_report.root, "z-link").state,
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
    let result = scan(fixture.path(), true, &cancelled, |event| {
        if let Event::Snapshot(report) = event {
            snapshots.push(report);
            cancelled.store(true, Ordering::Relaxed);
        }
    });

    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::Interrupted);
    assert_eq!(snapshots.len(), 1);
    assert!(snapshots[0].is_preview);
    Ok(())
}
