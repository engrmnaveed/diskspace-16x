//! Moves files and folders to the Trash, the only filesystem write in DiskUse16x. The
//! explorer calls it only after the user confirms. The scanners never write.
//!
//! Each backend moves an item within its own disk and never copies it, so trashing is
//! quick and an item that can't be moved stays where it is. Space is freed only when the
//! Trash is emptied, and trashing a hard link, a clone, or a snapshotted file may free
//! nothing at all.

#[cfg(any(not(target_os = "macos"), test))]
mod freedesktop;
#[cfg(target_os = "macos")]
mod macos;

use std::{
    env,
    ffi::OsStr,
    io,
    path::{Path, PathBuf},
};

/// Set to `1` to turn trashing off in the explorer, as `--read-only` does.
pub const READ_ONLY_VARIABLE: &str = "DISKUSE16X_READ_ONLY";

/// The variable's name before 1.0, still read when the new one isn't set.
const LEGACY_READ_ONLY_VARIABLE: &str = "SPACE16X_READ_ONLY";

/// True when `DISKUSE16X_READ_ONLY`, or else the earlier `SPACE16X_READ_ONLY`, holds any
/// value except an empty one or `0`.
pub fn read_only_requested() -> bool {
    read_only_from(
        env::var_os(READ_ONLY_VARIABLE).as_deref(),
        env::var_os(LEGACY_READ_ONLY_VARIABLE).as_deref(),
    )
}

fn read_only_from(current: Option<&OsStr>, legacy: Option<&OsStr>) -> bool {
    read_only_value(current.or(legacy))
}

fn read_only_value(value: Option<&OsStr>) -> bool {
    value.is_some_and(|value| !value.is_empty() && value != "0")
}

/// Moves `path` to the Trash on its disk and returns where it went. The last path
/// component itself is moved, never a symbolic link's target.
pub fn move_to_trash(path: &Path) -> io::Result<PathBuf> {
    #[cfg(target_os = "macos")]
    return macos::move_to_trash(path);
    #[cfg(not(target_os = "macos"))]
    return freedesktop::move_to_trash(path);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_FOLDER: AtomicU64 = AtomicU64::new(0);

    /// A scratch folder under the system temporary directory, removed on drop.
    pub(super) struct Scratch(pub(super) PathBuf);

    impl Scratch {
        pub(super) fn new() -> io::Result<Self> {
            let folder = env::temp_dir().join(format!(
                "diskuse16x-trash-test-{}-{}",
                std::process::id(),
                NEXT_FOLDER.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&folder)?;
            Ok(Self(fs::canonicalize(folder)?))
        }

        pub(super) fn join(&self, path: impl AsRef<Path>) -> PathBuf {
            self.0.join(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn read_only_variable_accepts_any_value_but_empty_or_zero() {
        assert!(!read_only_value(None));
        assert!(!read_only_value(Some(OsStr::new(""))));
        assert!(!read_only_value(Some(OsStr::new("0"))));
        assert!(read_only_value(Some(OsStr::new("1"))));
        assert!(read_only_value(Some(OsStr::new("yes"))));
        // The earlier name still works, and the new one decides when both are set.
        assert!(read_only_from(None, Some(OsStr::new("1"))));
        assert!(!read_only_from(
            Some(OsStr::new("0")),
            Some(OsStr::new("1"))
        ));
        assert!(read_only_from(Some(OsStr::new("1")), None));
        assert!(!read_only_from(None, None));
    }

    #[test]
    fn missing_items_are_reported_and_nothing_moves() -> io::Result<()> {
        let scratch = Scratch::new()?;
        let error = move_to_trash(&scratch.join("missing")).expect_err("nothing to trash");
        assert_eq!(error.kind(), io::ErrorKind::NotFound, "{error}");
        Ok(())
    }

    /// Trashes a scratch file in the real Trash, then removes it from there again.
    #[cfg(target_os = "macos")]
    #[test]
    fn moves_a_file_to_the_user_trash() -> io::Result<()> {
        let scratch = Scratch::new()?;
        let item = scratch.join("diskuse16x trash test ✓.txt");
        fs::write(&item, b"synthetic")?;
        let trashed = move_to_trash(&item)?;
        assert!(fs::symlink_metadata(&item).is_err());
        assert_eq!(trashed.extension(), item.extension());
        // Without Full Disk Access, macOS may hide the Trash's contents from this process.
        match fs::remove_file(&trashed) {
            Err(error) if error.raw_os_error() != Some(libc::EPERM) => Err(error),
            _ => Ok(()),
        }
    }
}
