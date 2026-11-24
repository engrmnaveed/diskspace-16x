//! The FreeDesktop.org Trash used by Linux desktops and file managers:
//! <https://specifications.freedesktop.org/trash-spec/latest/>
//!
//! An item goes to the home Trash when it is on the same device, and otherwise to
//! `$topdir/.Trash/$uid` or `$topdir/.Trash-$uid` at the top of its own mount. Its
//! `.trashinfo` record is written and synced before the item is renamed, so the Trash
//! never holds an item without its original location. Nothing is copied: an item with no
//! usable Trash on its device stays where it is.

use std::env;
use std::fmt::Write as _;
use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

/// The sticky bit, `S_ISVTX`, which lets only an item's owner remove it from a folder.
const STICKY: u32 = 0o1000;

/// How many numbered names to try when the Trash already holds items with this name.
const NAME_ATTEMPTS: u32 = 1_000;

// macOS builds this module only to test it, without its environment-based entry points.
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub fn move_to_trash(path: &Path) -> io::Result<PathBuf> {
    let home_trash = home_trash().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "Set HOME or XDG_DATA_HOME so DiskUse16x can find your Trash.",
        )
    })?;
    move_to_trash_with(path, &home_trash, unsafe { libc::geteuid() })
}

/// `$XDG_DATA_HOME/Trash`, which defaults to `~/.local/share/Trash`.
#[cfg_attr(target_os = "macos", allow(dead_code))]
fn home_trash() -> Option<PathBuf> {
    let absolute = |name| {
        env::var_os(name)
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
    };
    let data = absolute("XDG_DATA_HOME").or_else(|| Some(absolute("HOME")?.join(".local/share")));
    Some(data?.join("Trash"))
}

fn move_to_trash_with(path: &Path, home_trash: &Path, uid: u32) -> io::Result<PathBuf> {
    let item = resolve_parent(path)?;
    let device = fs::symlink_metadata(&item)?.dev();
    let trash = if nearest_device(home_trash)? == device {
        Trash::open(home_trash.to_path_buf(), None)?
    } else {
        Trash::at_top(&mount_top(&item, device)?, uid)?
    };
    if fs::metadata(&trash.files)?.dev() != device {
        return Err(cross_device());
    }
    trash.put(&item)
}

/// Resolves symbolic links in the parent folders but not in the item itself, which is
/// what gets moved. The resolved path finds the right mount and becomes the record.
fn resolve_parent(path: &Path) -> io::Result<PathBuf> {
    let name = path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Choose a file or folder to move to the Trash.",
        )
    })?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    Ok(fs::canonicalize(parent)?.join(name))
}

/// The device of `path`, or of its nearest existing ancestor if it doesn't exist yet.
fn nearest_device(path: &Path) -> io::Result<u64> {
    for ancestor in path.ancestors() {
        match fs::metadata(ancestor) {
            Ok(metadata) => return Ok(metadata.dev()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(io::ErrorKind::NotFound, "No Trash folder"))
}

/// The topmost folder above `item` that is still on `device`, the mount's root.
fn mount_top(item: &Path, device: u64) -> io::Result<PathBuf> {
    let mut top = item.parent().unwrap_or(Path::new("/")).to_path_buf();
    if fs::metadata(&top)?.dev() != device {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "DiskUse16x doesn't move mount points to the Trash.",
        ));
    }
    while let Some(parent) = top.parent() {
        if fs::metadata(parent)?.dev() != device {
            break;
        }
        top = parent.to_path_buf();
    }
    Ok(top)
}

fn cross_device() -> io::Error {
    io::Error::new(
        io::ErrorKind::CrossesDevices,
        "This disk has no Trash for the item, and DiskUse16x won't copy it to another disk.",
    )
}

struct Trash {
    files: PathBuf,
    info: PathBuf,
    /// A Trash at the top of a mount records original paths relative to that folder.
    top: Option<PathBuf>,
}

impl Trash {
    fn open(root: PathBuf, top: Option<PathBuf>) -> io::Result<Self> {
        let files = root.join("files");
        let info = root.join("info");
        private_folder(&files)?;
        private_folder(&info)?;
        Ok(Self { files, info, top })
    }

    /// Uses the administrator's shared `.Trash` if it is a real folder with the sticky
    /// bit, as the specification requires, and otherwise the user's own `.Trash-$uid`.
    fn at_top(top: &Path, uid: u32) -> io::Result<Self> {
        let shared = top.join(".Trash");
        let root = if fs::symlink_metadata(&shared)
            .is_ok_and(|m| m.file_type().is_dir() && m.mode() & STICKY != 0)
        {
            shared.join(uid.to_string())
        } else {
            top.join(format!(".Trash-{uid}"))
        };
        private_folder(&root).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("This disk has no Trash you can use ({error})."),
            )
        })?;
        // Another user could have planted the folder or a symbolic link in its place.
        let metadata = fs::symlink_metadata(&root)?;
        if !metadata.file_type().is_dir() || metadata.uid() != uid {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "This disk's Trash folder belongs to someone else.",
            ));
        }
        Self::open(root, Some(top.to_path_buf()))
    }

    /// Reserves a unique name with the `.trashinfo` record, then renames the item.
    fn put(&self, item: &Path) -> io::Result<PathBuf> {
        let name = item.file_name().unwrap_or_default().as_bytes();
        let original = self
            .top
            .as_deref()
            .and_then(|top| item.strip_prefix(top).ok())
            .unwrap_or(item);
        let record = format!(
            "[Trash Info]\nPath={}\nDeletionDate={}\n",
            encode(original),
            deletion_date()
        );
        for attempt in 1..=NAME_ATTEMPTS {
            let mut candidate = name.to_vec();
            if attempt > 1 {
                candidate.extend_from_slice(format!(".{attempt}").as_bytes());
            }
            let target = self
                .files
                .join(std::ffi::OsString::from_vec(candidate.clone()));
            candidate.extend_from_slice(b".trashinfo");
            let info_path = self.info.join(std::ffi::OsString::from_vec(candidate));
            let info = match OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&info_path)
            {
                Ok(info) => info,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            };
            match record_then_move(info, &record, item, &target) {
                Ok(()) => return Ok(target),
                Err(error) => {
                    let _ = fs::remove_file(&info_path);
                    if error.kind() != io::ErrorKind::AlreadyExists {
                        return Err(error);
                    }
                }
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "The Trash already holds too many items with this name.",
        ))
    }
}

fn record_then_move(mut info: File, record: &str, item: &Path, target: &Path) -> io::Result<()> {
    info.write_all(record.as_bytes())?;
    info.sync_all()?;
    // rename(2) would silently replace a file or an empty folder at the target.
    if fs::symlink_metadata(target).is_ok() {
        return Err(io::ErrorKind::AlreadyExists.into());
    }
    fs::rename(item, target).map_err(|error| {
        if error.kind() == io::ErrorKind::CrossesDevices {
            cross_device()
        } else {
            error
        }
    })
}

fn private_folder(path: &Path) -> io::Result<()> {
    DirBuilder::new().recursive(true).mode(0o700).create(path)
}

/// Percent-encodes a path as the specification's URL-style `Path` key expects.
fn encode(path: &Path) -> String {
    let mut encoded = String::new();
    for &byte in path.as_os_str().as_bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~/".contains(&byte) {
            encoded.push(byte.into());
        } else {
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

/// Local time as `YYYY-MM-DDThh:mm:ss`, the `DeletionDate` format.
fn deletion_date() -> String {
    // SAFETY: `tm` is plain data, and the calls only fill it.
    let tm = unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&now, &mut tm).is_null() {
            libc::gmtime_r(&now, &mut tm);
        }
        tm
    };
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec
    )
}

#[cfg(test)]
mod tests {
    use super::super::tests::Scratch;
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn uid() -> u32 {
        unsafe { libc::geteuid() }
    }

    fn record(trash: &Path, name: &str) -> String {
        fs::read_to_string(trash.join("info").join(format!("{name}.trashinfo"))).unwrap()
    }

    #[test]
    fn moves_files_and_folders_into_the_home_trash_with_records() -> io::Result<()> {
        let scratch = Scratch::new()?;
        let home_trash = scratch.join("data/Trash");
        fs::create_dir_all(scratch.join("work/nested"))?;
        fs::write(scratch.join("work/nested/report 100%.txt"), b"synthetic")?;
        let file = scratch.join("work/nested/report 100%.txt");

        let trashed = move_to_trash_with(&file, &home_trash, uid())?;
        assert_eq!(trashed, home_trash.join("files/report 100%.txt"));
        assert!(fs::symlink_metadata(&file).is_err());
        assert_eq!(fs::read(&trashed)?, b"synthetic");
        let info = record(&home_trash, "report 100%.txt");
        let path = encode(&scratch.join("work/nested"));
        assert!(info.starts_with("[Trash Info]\n"), "{info}");
        assert!(
            info.contains(&format!("\nPath={path}/report%20100%25.txt\n")),
            "{info}"
        );
        let date = info.split("DeletionDate=").nth(1).unwrap().trim_end();
        assert_eq!(date.len(), 19, "{date}");
        assert_eq!(&date[10..11], "T");
        let mode = fs::metadata(home_trash.join("files"))?.permissions().mode();
        assert_eq!(mode & 0o077, 0, "the Trash is private");

        let folder = scratch.join("work/nested");
        let trashed = move_to_trash_with(&folder, &home_trash, uid())?;
        assert_eq!(trashed, home_trash.join("files/nested"));
        assert!(trashed.is_dir());
        Ok(())
    }

    #[test]
    fn numbers_names_already_in_the_trash() -> io::Result<()> {
        let scratch = Scratch::new()?;
        let home_trash = scratch.join("Trash");
        for expected in ["notes", "notes.2", "notes.3"] {
            fs::write(scratch.join("notes"), expected)?;
            let trashed = move_to_trash_with(&scratch.join("notes"), &home_trash, uid())?;
            assert_eq!(trashed, home_trash.join("files").join(expected));
            assert_eq!(fs::read_to_string(&trashed)?, expected);
        }
        // An item without a record still keeps its name from being reused.
        fs::write(home_trash.join("files/orphan"), b"")?;
        fs::write(scratch.join("orphan"), b"new")?;
        let trashed = move_to_trash_with(&scratch.join("orphan"), &home_trash, uid())?;
        assert_eq!(trashed, home_trash.join("files/orphan.2"));
        assert!(!home_trash.join("info/orphan.trashinfo").exists());
        Ok(())
    }

    #[test]
    fn moves_a_symbolic_link_rather_than_its_target() -> io::Result<()> {
        let scratch = Scratch::new()?;
        fs::write(scratch.join("target"), b"keep")?;
        std::os::unix::fs::symlink(scratch.join("target"), scratch.join("link"))?;
        let trashed = move_to_trash_with(&scratch.join("link"), &scratch.join("Trash"), uid())?;
        assert!(fs::symlink_metadata(&trashed)?.file_type().is_symlink());
        assert_eq!(fs::read(scratch.join("target"))?, b"keep");
        Ok(())
    }

    #[test]
    fn removes_the_record_when_the_move_fails() -> io::Result<()> {
        let scratch = Scratch::new()?;
        let home_trash = scratch.join("Trash");
        fs::create_dir(scratch.join("locked"))?;
        fs::write(scratch.join("locked/item"), b"stay")?;
        fs::set_permissions(scratch.join("locked"), fs::Permissions::from_mode(0o500))?;
        let result = move_to_trash_with(&scratch.join("locked/item"), &home_trash, uid());
        fs::set_permissions(scratch.join("locked"), fs::Permissions::from_mode(0o700))?;
        if uid() == 0 {
            return Ok(()); // root ignores the folder's permissions
        }
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(fs::read(scratch.join("locked/item"))?, b"stay");
        assert_eq!(fs::read_dir(home_trash.join("info"))?.count(), 0);
        Ok(())
    }

    #[test]
    fn top_of_mount_trash_prefers_a_sticky_shared_folder() -> io::Result<()> {
        let scratch = Scratch::new()?;
        let top = scratch.0.clone();
        fs::write(scratch.join("item"), b"")?;

        let own = Trash::at_top(&top, uid())?;
        assert_eq!(own.files, top.join(format!(".Trash-{}/files", uid())));
        let trashed = own.put(&scratch.join("item"))?;
        assert_eq!(
            fs::read_to_string(own.info.join("item.trashinfo"))?
                .lines()
                .nth(1),
            Some("Path=item")
        );
        assert!(trashed.exists());

        // A shared folder without the sticky bit could let other users remove items.
        fs::create_dir(top.join(".Trash"))?;
        assert_eq!(Trash::at_top(&top, uid())?.files, own.files);
        fs::set_permissions(top.join(".Trash"), fs::Permissions::from_mode(0o1777))?;
        let shared = Trash::at_top(&top, uid())?;
        assert_eq!(shared.files, top.join(format!(".Trash/{}/files", uid())));
        Ok(())
    }

    #[test]
    fn rejects_paths_without_a_name() {
        for path in ["/", "..", "a/.."] {
            let error = resolve_parent(Path::new(path)).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput, "{path}");
        }
    }

    #[test]
    fn encodes_reserved_bytes_and_keeps_separators() {
        let path = Path::new(std::ffi::OsStr::from_bytes(b"/a b/%/\xff/ok-_.~"));
        assert_eq!(encode(path), "/a%20b/%25/%FF/ok-_.~");
    }
}
