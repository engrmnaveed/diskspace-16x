use std::ffi::{CStr, OsStr};
use std::io;
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

const DATA_MOUNT: &str = "/System/Volumes/Data";

pub(super) struct StartupVolume {
    canonical_scope: PathBuf,
    backing_mount: PathBuf,
}

impl StartupVolume {
    pub(super) fn for_scope(scope: &Path, fd: &OwnedFd) -> Option<Self> {
        let canonical = if scope == Path::new("/") {
            scope.to_path_buf()
        } else {
            canonical_path_at(fd, OsStr::new(".")).ok()?
        };
        Self::new(scope, canonical)
    }

    fn new(scope: &Path, canonical_scope: PathBuf) -> Option<Self> {
        let relative_mount = Path::new(DATA_MOUNT).strip_prefix(&canonical_scope).ok()?;
        // A scan started at or inside the Data mount treats it as its own scope.
        if relative_mount.as_os_str().is_empty() {
            return None;
        }
        let backing_mount = scope.join(relative_mount);
        Some(Self {
            canonical_scope,
            backing_mount,
        })
    }

    pub(super) fn is_backing_path(&self, path: &Path) -> bool {
        path != self.backing_mount && path.starts_with(&self.backing_mount)
    }

    pub(super) fn is_alias(&self, canonical: &Path) -> bool {
        // Keep firmlinks in their visible locations and retain Data-only directories.
        canonical != self.canonical_scope
            && canonical.starts_with(&self.canonical_scope)
            && !canonical.starts_with(DATA_MOUNT)
    }
}

pub(super) fn canonical_path_at(parent: &OwnedFd, name: &OsStr) -> io::Result<PathBuf> {
    // ATTR_CMN_FULLPATH reports the visible path even when reached through the Data mount.
    // The only requested attribute is a path reference immediately after the buffer length.
    #[repr(C)]
    struct PathAttributes {
        length: u32,
        reference: libc::attrreference_t,
        path: [u8; libc::PATH_MAX as usize],
    }

    let name = super::fs::posix_path(name)?;
    let mut request = libc::attrlist {
        bitmapcount: libc::ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: libc::ATTR_CMN_FULLPATH,
        volattr: 0,
        dirattr: 0,
        fileattr: 0,
        forkattr: 0,
    };
    let mut response = PathAttributes {
        length: 0,
        reference: libc::attrreference_t {
            attr_dataoffset: 0,
            attr_length: 0,
        },
        path: [0; libc::PATH_MAX as usize],
    };
    // SAFETY: the descriptor is borrowed, the name is NUL-terminated, and both structs are writable.
    let status = unsafe {
        libc::getattrlistat(
            parent.as_raw_fd(),
            name.as_ptr(),
            std::ptr::from_mut(&mut request).cast(),
            std::ptr::from_mut(&mut response).cast(),
            std::mem::size_of_val(&response),
            libc::FSOPT_NOFOLLOW.into(),
        )
    };
    if status != 0 {
        return Err(io::Error::last_os_error());
    }
    let path_header = std::mem::size_of::<libc::attrreference_t>();
    if response.reference.attr_dataoffset as usize != path_header {
        return Err(io::Error::other(
            "macOS returned an invalid path reference.",
        ));
    }
    let path = response
        .path
        .get(..response.reference.attr_length as usize)
        .and_then(|bytes| CStr::from_bytes_with_nul(bytes).ok())
        .map(|path| Path::new(OsStr::from_bytes(path.to_bytes())))
        .filter(|path| path.is_absolute())
        .ok_or_else(|| io::Error::other("macOS returned an invalid canonical path."))?;
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excludes_only_backing_paths_with_a_visible_alias_inside_the_scan() {
        let scope = Path::new("/test/System/..");
        let volume = StartupVolume::new(scope, "/".into()).unwrap();
        assert!(volume.is_backing_path(&scope.join("System/Volumes/Data/Users")));
        assert!(volume.is_backing_path(&scope.join("System/Volumes/Data/System/Library/Caches")));
        assert!(!volume.is_backing_path(&scope.join("System/Volumes/Data")));
        assert!(!volume.is_backing_path(&scope.join("System/Volumes/DataBackup/Users")));
        assert!(!volume.is_backing_path(&scope.join("Users")));
        assert!(volume.is_alias(Path::new("/Users")));
        assert!(volume.is_alias(Path::new("/System/Library/Caches")));
        assert!(!volume.is_alias(Path::new("/System/Volumes/Data/.fseventsd")));

        let system = StartupVolume::new(Path::new("/System"), "/System".into()).unwrap();
        assert!(system.is_alias(Path::new("/System/Library/Caches")));
        assert!(!system.is_alias(Path::new("/Users")));
        assert!(!system.is_alias(Path::new("/SystemBackup/Library")));
        assert!(StartupVolume::new(Path::new(DATA_MOUNT), DATA_MOUNT.into()).is_none());
    }

    #[test]
    fn reports_visible_paths_for_firmlinks_on_the_startup_disk() -> io::Result<()> {
        use std::os::unix::fs::MetadataExt;

        let data = Path::new(DATA_MOUNT);
        let (Ok(visible), Ok(backing)) = (
            std::fs::symlink_metadata("/Users"),
            std::fs::symlink_metadata(data.join("Users")),
        ) else {
            return Ok(());
        };
        // Without an APFS volume group the two paths are unrelated directories.
        if (visible.dev(), visible.ino()) != (backing.dev(), backing.ino()) {
            return Ok(());
        }
        let mount = OwnedFd::from(std::fs::File::open(data)?);
        assert_eq!(
            canonical_path_at(&mount, OsStr::new("Users"))?,
            Path::new("/Users")
        );
        assert_eq!(canonical_path_at(&mount, OsStr::new("."))?, data);
        Ok(())
    }
}
