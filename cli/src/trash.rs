//! Moves files and folders to the Trash, the only filesystem write in Space 16X. The
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
    io,
    path::{Path, PathBuf},
};

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
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::{env, fs};

    static NEXT_FOLDER: AtomicU64 = AtomicU64::new(0);

    /// A scratch folder under the system temporary directory, removed on drop.
    pub(super) struct Scratch(pub(super) PathBuf);

    impl Scratch {
        pub(super) fn new() -> io::Result<Self> {
            let folder = env::temp_dir().join(format!(
                "space16x-trash-test-{}-{}",
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
        let item = scratch.join("space16x trash test ✓.txt");
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
