//! Used and free space on each mounted volume, from `statvfs` on Linux and `statfs` on
//! macOS, whose `statvfs` counts blocks in 32 bits.

#[cfg(target_os = "macos")]
use std::ffi::CStr;
use std::ffi::{CString, OsStr};
use std::io::{self, Write};
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

use serde::Serialize;

use crate::report::{self, JsonPath};

#[derive(Debug, PartialEq, Eq)]
pub struct Volume {
    pub path: PathBuf,
    pub device: String,
    pub kind: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    /// Free to unprivileged users, which excludes blocks reserved for root.
    pub available_bytes: u64,
    pub total_inodes: u64,
    pub free_inodes: u64,
    pub read_only: bool,
}

impl Volume {
    /// Used space as a share of the space this user can use, rounded up, as `df` does.
    fn used_percent(&self) -> u64 {
        percent(
            self.used_bytes,
            self.used_bytes.saturating_add(self.available_bytes),
        )
    }

    fn inode_percent(&self) -> Option<u64> {
        (self.total_inodes > 0).then(|| {
            percent(
                self.total_inodes.saturating_sub(self.free_inodes),
                self.total_inodes,
            )
        })
    }
}

fn percent(part: u64, whole: u64) -> u64 {
    if whole == 0 {
        return 0;
    }
    ((u128::from(part) * 100).div_ceil(u128::from(whole))) as u64
}

/// A mount from the system's mount table.
pub(crate) struct Mount {
    pub(crate) path: PathBuf,
    pub(crate) device: String,
    pub(crate) kind: String,
    /// Shown by default. Hidden mounts are system volumes, memory filesystems, snap
    /// images, and bind mounts of a device already listed.
    shown: bool,
}

/// A mount that couldn't be read, and why.
pub type Issue = (PathBuf, String);

/// Mounted volumes, with an issue for each one that couldn't be read. `all` includes
/// hidden and empty filesystems.
pub fn list(all: bool) -> io::Result<(Vec<Volume>, Vec<Issue>)> {
    let mut volumes = Vec::new();
    let mut issues = Vec::new();
    for mount in mounts()? {
        if !(all || mount.shown) {
            continue;
        }
        match usage(mount) {
            Ok(volume) if all || volume.total_bytes > 0 => volumes.push(volume),
            Ok(_) => {}
            Err((path, error)) => issues.push((path, error.to_string())),
        }
    }
    Ok((volumes, issues))
}

#[cfg(target_os = "linux")]
pub(crate) fn mounts() -> io::Result<Vec<Mount>> {
    let table = std::fs::read("/proc/self/mounts")?;
    let mut devices = Vec::new();
    let mut mounts = Vec::new();
    for line in table.split(|&byte| byte == b'\n') {
        let mut fields = line.split(|&byte| byte == b' ').map(unescape);
        let (Some(device), Some(path), Some(kind)) = (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let path = PathBuf::from(OsStr::from_bytes(&path));
        // A bind mount repeats a device that is already listed.
        let id = std::fs::metadata(&path).map(|metadata| {
            use std::os::unix::fs::MetadataExt;
            metadata.dev()
        });
        let repeated = id.as_ref().is_ok_and(|id| devices.contains(id));
        if let Ok(id) = id {
            devices.push(id);
        }
        let memory = matches!(&kind[..], b"tmpfs" | b"devtmpfs" | b"ramfs" | b"squashfs");
        mounts.push(Mount {
            path,
            device: String::from_utf8_lossy(&device).into_owned(),
            kind: String::from_utf8_lossy(&kind).into_owned(),
            shown: !repeated && !memory,
        });
    }
    Ok(mounts)
}

/// Decodes the octal escapes the kernel writes for spaces, tabs, newlines and backslashes.
#[cfg(target_os = "linux")]
fn unescape(field: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(field.len());
    let mut index = 0;
    while index < field.len() {
        let digits = field.get(index + 1..index + 4);
        if field[index] == b'\\'
            && let Some(code) = digits
                .filter(|digits| digits.iter().all(|digit| (b'0'..=b'7').contains(digit)))
                .and_then(|digits| u8::from_str_radix(std::str::from_utf8(digits).ok()?, 8).ok())
        {
            output.push(code);
            index += 4;
        } else {
            output.push(field[index]);
            index += 1;
        }
    }
    output
}

#[cfg(target_os = "linux")]
pub(crate) fn usage(mount: Mount) -> Result<Volume, (PathBuf, io::Error)> {
    let path = CString::new(mount.path.as_os_str().as_bytes())
        .map_err(|error| (mount.path.clone(), io::Error::other(error)))?;
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: the path is a C string, and statvfs fills the buffer when it returns 0.
    if unsafe { libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) } != 0 {
        return Err((mount.path, io::Error::last_os_error()));
    }
    // SAFETY: statvfs returned 0.
    let stats = unsafe { stats.assume_init() };
    let block = stats.f_frsize;
    Ok(Volume {
        total_bytes: stats.f_blocks.saturating_mul(block),
        used_bytes: stats
            .f_blocks
            .saturating_sub(stats.f_bfree)
            .saturating_mul(block),
        available_bytes: stats.f_bavail.saturating_mul(block),
        total_inodes: stats.f_files,
        free_inodes: stats.f_ffree,
        read_only: stats.f_flag & libc::ST_RDONLY != 0,
        path: mount.path,
        device: mount.device,
        kind: mount.kind,
    })
}

#[cfg(target_os = "macos")]
pub(crate) fn mounts() -> io::Result<Vec<Mount>> {
    let mut table = std::ptr::null_mut();
    // SAFETY: getmntinfo points `table` at its own buffer of `count` entries, which
    // stays valid until the next call.
    let count = unsafe { libc::getmntinfo(&mut table, libc::MNT_NOWAIT) };
    if count <= 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: as above.
    let table = unsafe { std::slice::from_raw_parts(table, count as usize) };
    Ok(table
        .iter()
        .map(|mount| {
            let text = |field: &[libc::c_char]| {
                // SAFETY: the kernel terminates these fixed-size fields with a NUL.
                unsafe { CStr::from_ptr(field.as_ptr()) }
            };
            let path = PathBuf::from(OsStr::from_bytes(text(&mount.f_mntonname).to_bytes()));
            // The startup disk's data volume is hidden, but it holds the user's files.
            let hidden = mount.f_flags & libc::MNT_DONTBROWSE as u32 != 0
                && path.as_os_str() != "/System/Volumes/Data";
            Mount {
                path,
                device: text(&mount.f_mntfromname).to_string_lossy().into_owned(),
                kind: text(&mount.f_fstypename).to_string_lossy().into_owned(),
                shown: !hidden,
            }
        })
        .collect())
}

#[cfg(target_os = "macos")]
pub(crate) fn usage(mount: Mount) -> Result<Volume, (PathBuf, io::Error)> {
    let path = CString::new(mount.path.as_os_str().as_bytes())
        .map_err(|error| (mount.path.clone(), io::Error::other(error)))?;
    let mut stats = std::mem::MaybeUninit::<libc::statfs>::uninit();
    // SAFETY: the path is a C string, and statfs fills the buffer when it returns 0.
    if unsafe { libc::statfs(path.as_ptr(), stats.as_mut_ptr()) } != 0 {
        return Err((mount.path, io::Error::last_os_error()));
    }
    // SAFETY: statfs returned 0.
    let stats = unsafe { stats.assume_init() };
    let block = u64::from(stats.f_bsize);
    Ok(Volume {
        total_bytes: stats.f_blocks.saturating_mul(block),
        used_bytes: stats
            .f_blocks
            .saturating_sub(stats.f_bfree)
            .saturating_mul(block),
        available_bytes: stats.f_bavail.saturating_mul(block),
        total_inodes: stats.f_files,
        free_inodes: stats.f_ffree,
        read_only: stats.f_flags & libc::MNT_RDONLY as u32 != 0,
        path: mount.path,
        device: mount.device,
        kind: mount.kind,
    })
}

pub fn write_plain(out: impl Write, volumes: &[Volume]) -> io::Result<()> {
    let mut out = io::BufWriter::new(out);
    writeln!(
        out,
        "{:>10} {:>10} {:>10}  {:>4}  {:>6}  {:<6}  Mounted on",
        "Size", "Used", "Free", "Use%", "Inodes", "Type"
    )?;
    for volume in volumes {
        let inodes = volume
            .inode_percent()
            .map_or_else(|| "-".into(), |percent| format!("{percent}%"));
        writeln!(
            out,
            "{:>10} {:>10} {:>10}  {:>3}%  {inodes:>6}  {:<6}  {}",
            report::format_bytes(volume.total_bytes),
            report::format_bytes(volume.used_bytes),
            report::format_bytes(volume.available_bytes),
            volume.used_percent(),
            report::terminal_text(&volume.kind),
            report::terminal_path(&volume.path),
        )?;
    }
    out.flush()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonVolumes<'a> {
    schema_version: u8,
    edition: &'static str,
    volumes: Vec<JsonVolume<'a>>,
    issues: Vec<JsonIssue<'a>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonVolume<'a> {
    #[serde(flatten)]
    path: JsonPath<'a>,
    device: &'a str,
    #[serde(rename = "type")]
    kind: &'a str,
    total_bytes: u64,
    used_bytes: u64,
    available_bytes: u64,
    total_inodes: u64,
    free_inodes: u64,
    read_only: bool,
}

#[derive(Serialize)]
struct JsonIssue<'a> {
    #[serde(flatten)]
    path: JsonPath<'a>,
    message: &'a str,
}

pub fn write_json(out: impl Write, volumes: &[Volume], issues: &[Issue]) -> io::Result<()> {
    let mut out = io::BufWriter::new(out);
    let json = JsonVolumes {
        schema_version: 3,
        edition: report::EDITION,
        volumes: volumes
            .iter()
            .map(|volume| JsonVolume {
                path: JsonPath::from(volume.path.as_path()),
                device: &volume.device,
                kind: &volume.kind,
                total_bytes: volume.total_bytes,
                used_bytes: volume.used_bytes,
                available_bytes: volume.available_bytes,
                total_inodes: volume.total_inodes,
                free_inodes: volume.free_inodes,
                read_only: volume.read_only,
            })
            .collect(),
        issues: issues
            .iter()
            .map(|(path, message)| JsonIssue {
                path: JsonPath::from(path.as_path()),
                message,
            })
            .collect(),
    };
    serde_json::to_writer_pretty(&mut out, &json)?;
    writeln!(out)?;
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn volume(path: &str, total: u64, used: u64, available: u64, inodes: (u64, u64)) -> Volume {
        Volume {
            path: path.into(),
            device: "/dev/test".into(),
            kind: "ext4".into(),
            total_bytes: total,
            used_bytes: used,
            available_bytes: available,
            total_inodes: inodes.0,
            free_inodes: inodes.1,
            read_only: false,
        }
    }

    #[test]
    fn rounds_usage_up_like_df() {
        // 5% of ext4 is reserved for root, so used and available don't add up to the size.
        let disk = volume("/", 100_000, 40_000, 55_000, (1000, 999));
        assert_eq!(disk.used_percent(), 43);
        assert_eq!(disk.inode_percent(), Some(1));
        let empty = volume("/proc", 0, 0, 0, (0, 0));
        assert_eq!(empty.used_percent(), 0);
        assert_eq!(empty.inode_percent(), None);
        assert_eq!(percent(u64::MAX, u64::MAX), 100);
    }

    #[test]
    fn prints_a_table_and_json() {
        let volumes = [volume(
            "/mnt/my disk",
            2_000_000_000,
            1_500_000_000,
            400_000_000,
            (0, 0),
        )];
        let mut plain = Vec::new();
        write_plain(&mut plain, &volumes).unwrap();
        assert_eq!(
            String::from_utf8(plain).unwrap(),
            "      Size       Used       Free  Use%  Inodes  Type    Mounted on\n   \
             2.00 GB    1.50 GB   400.0 MB   79%       -  ext4    /mnt/my disk\n"
        );
        let mut json = Vec::new();
        write_json(
            &mut json,
            &volumes,
            &[("/mnt/gone".into(), "Stale file handle".into())],
        )
        .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&json).unwrap();
        assert_eq!(json["schemaVersion"], 3);
        assert_eq!(json["volumes"][0]["path"], "/mnt/my disk");
        assert_eq!(json["volumes"][0]["type"], "ext4");
        assert_eq!(json["volumes"][0]["availableBytes"], 400_000_000);
        assert_eq!(json["issues"][0]["path"], "/mnt/gone");
    }

    #[test]
    fn lists_the_root_volume() {
        let (volumes, _) = list(false).unwrap();
        let root = volumes
            .iter()
            .find(|volume| volume.path.as_os_str() == "/")
            .expect("the root volume is listed");
        assert!(root.total_bytes > 0 && root.used_bytes <= root.total_bytes);
        let (everything, _) = list(true).unwrap();
        assert!(everything.len() >= volumes.len());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn decodes_mount_table_escapes() {
        assert_eq!(unescape(br"/mnt/my\040disk\134x\011"), b"/mnt/my disk\\x\t");
        assert_eq!(unescape(br"\04"), br"\04");
        assert_eq!(unescape(br"\999"), br"\999");
    }
}
