//! `space16x explain`: the space on a volume that a scan can't show, from system queries
//! and metadata alone. It reads no file contents and changes nothing.
//!
//! On macOS that is the other volumes of the APFS container, which share its space,
//! purgeable space that macOS frees on demand, and Time Machine local snapshots. On Linux
//! it is blocks reserved for root and deleted files that programs still hold open.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::report::{self, JsonPath, format_bytes, terminal_path, terminal_text};
use crate::volumes::{self, Volume};

pub struct Explanation {
    volume: Volume,
    /// Free space that ordinary users can't use, which `df` counts as neither used nor free.
    reserved: u64,
    /// macOS: space the system frees when it's needed. None where unknown.
    purgeable: Option<u64>,
    /// macOS: local snapshot names. None where they couldn't be listed.
    snapshots: Option<Vec<String>>,
    /// macOS: the mounted volumes of the APFS container and their used space, largest first.
    container: Vec<(PathBuf, u64)>,
    /// Linux: deleted files that programs still hold open, largest first.
    open_files: Vec<OpenFile>,
    /// Linux: processes whose open files this user can't see.
    hidden_processes: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub struct OpenFile {
    path: PathBuf,
    allocated: u64,
    apparent: u64,
    pid: u32,
    program: String,
}

pub fn run(path: &Path, json: bool) -> io::Result<u8> {
    let explanation = explain(path).map_err(|error| crate::scan_error(path, error))?;
    let stdout = io::stdout().lock();
    if json {
        write_json(stdout, &explanation)?;
    } else {
        write_plain(stdout, &explanation)?;
        if explanation.hidden_processes > 0 {
            eprintln!(
                "space16x: the open files of {} other users' processes are hidden; run as root to include them",
                explanation.hidden_processes
            );
        }
    }
    Ok(0)
}

/// The text report for PATH's volume, line by line, for the explorer's Explain panel.
pub fn lines(path: &Path) -> Vec<String> {
    let explanation = match explain(path) {
        Ok(explanation) => explanation,
        Err(error) => return vec![format!("Couldn't explain this volume: {error}")],
    };
    let mut text = Vec::new();
    if let Err(error) = write_plain(&mut text, &explanation) {
        return vec![format!("Couldn't explain this volume: {error}")];
    }
    let mut lines: Vec<String> = String::from_utf8_lossy(&text)
        .lines()
        .map(str::to_owned)
        .collect();
    if explanation.hidden_processes > 0 {
        lines.push(String::new());
        lines.push(format!(
            "The open files of {} other users' processes are hidden.",
            explanation.hidden_processes
        ));
    }
    lines
}

fn explain(path: &Path) -> io::Result<Explanation> {
    let path = std::fs::canonicalize(path)?;
    let volume = containing_volume(&path)?;
    let reserved = volume
        .total_bytes
        .saturating_sub(volume.used_bytes)
        .saturating_sub(volume.available_bytes);
    let mut explanation = Explanation {
        volume,
        reserved,
        purgeable: None,
        snapshots: None,
        container: Vec::new(),
        open_files: Vec::new(),
        hidden_processes: 0,
    };
    #[cfg(target_os = "macos")]
    {
        let volume = &explanation.volume;
        explanation.purgeable = macos::important_capacity(&volume.path)
            .map(|important| important.saturating_sub(volume.available_bytes));
        explanation.snapshots = macos::snapshots(&volume.path);
        if volume.kind == "apfs" {
            explanation.container = macos::container_volumes(&volume.device)?;
        }
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::MetadataExt;
        let device = std::fs::metadata(&explanation.volume.path)?.dev();
        (explanation.open_files, explanation.hidden_processes) = linux::open_deleted(device);
    }
    Ok(explanation)
}

/// The mounted volume that holds `path`.
fn containing_volume(path: &Path) -> io::Result<Volume> {
    let mount_point = mount_point(path)?;
    let mount = volumes::mounts()?
        .into_iter()
        .rev()
        .find(|mount| mount.path == mount_point)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Its volume isn't mounted."))?;
    volumes::usage(mount).map_err(|(_, error)| error)
}

/// `statfs` names the mount point, which on macOS can differ from the path's prefix, as
/// for the firmlinked `/Users` on the data volume.
#[cfg(target_os = "macos")]
fn mount_point(path: &Path) -> io::Result<PathBuf> {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(path.as_os_str().as_bytes()).map_err(io::Error::other)?;
    let mut stats = std::mem::MaybeUninit::<libc::statfs>::uninit();
    // SAFETY: the path is a C string, and statfs fills the buffer when it returns 0.
    if unsafe { libc::statfs(path.as_ptr(), stats.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: statfs returned 0, and the kernel terminates the name with a NUL.
    let name = unsafe { std::ffi::CStr::from_ptr(stats.assume_init_ref().f_mntonname.as_ptr()) };
    Ok(PathBuf::from(std::ffi::OsStr::from_bytes(name.to_bytes())))
}

/// The deepest mount above `path` on the same device.
#[cfg(target_os = "linux")]
fn mount_point(path: &Path) -> io::Result<PathBuf> {
    use std::os::unix::fs::MetadataExt;
    let device = std::fs::metadata(path)?.dev();
    volumes::mounts()?
        .into_iter()
        .filter(|mount| path.starts_with(&mount.path))
        .filter(|mount| std::fs::metadata(&mount.path).is_ok_and(|m| m.dev() == device))
        .map(|mount| mount.path)
        .max_by_key(|mount| mount.as_os_str().len())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Its volume isn't mounted."))
}

/// The date in a Time Machine snapshot's name, as in
/// `com.apple.TimeMachine.2026-11-21-101500.local`.
fn snapshot_date(name: &str) -> Option<String> {
    let stamp = name
        .strip_prefix("com.apple.TimeMachine.")?
        .strip_suffix(".local")?;
    let bytes = stamp.as_bytes();
    let valid = bytes.len() == 17
        && bytes.iter().enumerate().all(|(index, byte)| match index {
            4 | 7 | 10 => *byte == b'-',
            _ => byte.is_ascii_digit(),
        });
    valid.then(|| format!("{} {}:{}", &stamp[..10], &stamp[11..13], &stamp[13..15]))
}

/// Snapshot names from `tmutil listlocalsnapshots`, which starts with a heading line.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn parse_snapshots(output: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(output)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.ends_with(':'))
        .map(str::to_owned)
        .collect()
}

pub fn write_plain(out: impl Write, explanation: &Explanation) -> io::Result<()> {
    let mut out = io::BufWriter::new(out);
    let volume = &explanation.volume;
    writeln!(
        out,
        "Volume      {} ({}, {})",
        terminal_path(&volume.path),
        terminal_text(&volume.kind),
        terminal_text(&volume.device)
    )?;
    writeln!(out, "Size        {}", format_bytes(volume.total_bytes))?;
    let shared = if explanation.container.len() > 1 {
        ", shared by the volumes of its APFS container"
    } else {
        ""
    };
    writeln!(
        out,
        "Used        {}{shared}",
        format_bytes(volume.used_bytes)
    )?;
    writeln!(out, "Free        {}", format_bytes(volume.available_bytes))?;
    if explanation.reserved > 0 {
        writeln!(
            out,
            "Reserved    {} for root, which df counts as neither used nor free",
            format_bytes(explanation.reserved)
        )?;
    }
    if let Some(purgeable) = explanation.purgeable {
        writeln!(
            out,
            "Purgeable   {} of caches, iCloud copies and snapshots that macOS frees when needed",
            format_bytes(purgeable)
        )?;
    }
    if cfg!(target_os = "macos") {
        let snapshots = match explanation.snapshots.as_deref() {
            None => "unknown: tmutil couldn't list them".to_owned(),
            Some([]) => "none".to_owned(),
            Some(names) => {
                let dates: Vec<String> = names.iter().filter_map(|n| snapshot_date(n)).collect();
                let range = match (dates.iter().min(), dates.iter().max()) {
                    (Some(first), Some(last)) if first != last => format!(", {first} to {last}"),
                    (Some(only), _) => format!(", {only}"),
                    _ => String::new(),
                };
                format!("{} local{range}, holding deleted files' space", names.len())
            }
        };
        writeln!(out, "Snapshots   {snapshots}")?;
    }
    if explanation.container.len() > 1 {
        writeln!(out, "\n{:>10}  APFS volume", "Used")?;
        for (path, used) in &explanation.container {
            writeln!(out, "{:>10}  {}", format_bytes(*used), terminal_path(path))?;
        }
    }
    if cfg!(target_os = "linux") {
        let files = &explanation.open_files;
        let total = files.iter().map(|file| file.allocated).sum::<u64>();
        writeln!(
            out,
            "Open files  {} in {} deleted {} that programs still hold open",
            format_bytes(total),
            files.len(),
            if files.len() == 1 { "file" } else { "files" }
        )?;
        if !files.is_empty() {
            writeln!(
                out,
                "\n{:>10}  {:>8}  {:<15}  Path",
                "Size", "PID", "Program"
            )?;
            for file in files {
                writeln!(
                    out,
                    "{:>10}  {:>8}  {:<15}  {}",
                    format_bytes(file.allocated),
                    file.pid,
                    terminal_text(&file.program),
                    terminal_path(&file.path)
                )?;
            }
            writeln!(
                out,
                "\nPrograms free this space when they close these files or exit."
            )?;
        }
    }
    out.flush()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonExplanation<'a> {
    schema_version: u8,
    edition: &'static str,
    volume: JsonVolume<'a>,
    reserved_bytes: u64,
    purgeable_bytes: Option<u64>,
    snapshots: Option<Vec<JsonSnapshot<'a>>>,
    container_volumes: Vec<JsonContainerVolume<'a>>,
    deleted_open_files: Vec<JsonOpenFile<'a>>,
    hidden_processes: usize,
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
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonSnapshot<'a> {
    name: &'a str,
    /// The local date and time in a Time Machine snapshot's name.
    date: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonContainerVolume<'a> {
    #[serde(flatten)]
    path: JsonPath<'a>,
    used_bytes: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonOpenFile<'a> {
    #[serde(flatten)]
    path: JsonPath<'a>,
    allocated_bytes: u64,
    apparent_bytes: u64,
    pid: u32,
    program: &'a str,
}

pub fn write_json(out: impl Write, explanation: &Explanation) -> io::Result<()> {
    let mut out = io::BufWriter::new(out);
    let volume = &explanation.volume;
    let json = JsonExplanation {
        schema_version: 3,
        edition: report::EDITION,
        volume: JsonVolume {
            path: JsonPath::from(volume.path.as_path()),
            device: &volume.device,
            kind: &volume.kind,
            total_bytes: volume.total_bytes,
            used_bytes: volume.used_bytes,
            available_bytes: volume.available_bytes,
        },
        reserved_bytes: explanation.reserved,
        purgeable_bytes: explanation.purgeable,
        snapshots: explanation.snapshots.as_ref().map(|names| {
            names
                .iter()
                .map(|name| JsonSnapshot {
                    name,
                    date: snapshot_date(name),
                })
                .collect()
        }),
        container_volumes: explanation
            .container
            .iter()
            .map(|(path, used)| JsonContainerVolume {
                path: JsonPath::from(path.as_path()),
                used_bytes: *used,
            })
            .collect(),
        deleted_open_files: explanation
            .open_files
            .iter()
            .map(|file| JsonOpenFile {
                path: JsonPath::from(file.path.as_path()),
                allocated_bytes: file.allocated,
                apparent_bytes: file.apparent,
                pid: file.pid,
                program: &file.program,
            })
            .collect(),
        hidden_processes: explanation.hidden_processes,
    };
    serde_json::to_writer_pretty(&mut out, &json)?;
    writeln!(out)?;
    out.flush()
}

#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::{CString, c_char};
    use std::io;
    use std::os::unix::ffi::OsStrExt;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::ptr;

    use crate::objc::{ObjcBool, Object, file_url, objc_getClass, send, with_pool};
    use crate::volumes;

    /// Free space plus the purgeable space macOS would free for an important task, from
    /// `NSURLVolumeAvailableCapacityForImportantUsageKey`. None when macOS doesn't say,
    /// as for volumes other than the startup disk's.
    pub fn important_capacity(path: &Path) -> Option<u64> {
        let representation = CString::new(path.as_os_str().as_bytes()).ok()?;
        // SAFETY: Foundation's objects are released by the pool, and each message matches
        // its method's signature.
        with_pool(|| unsafe {
            let url = file_url(&representation);
            if url.is_null() {
                return None;
            }
            let key = send!(
                objc_getClass(c"NSString".as_ptr()),
                c"stringWithUTF8String:",
                c"NSURLVolumeAvailableCapacityForImportantUsageKey".as_ptr() => *const c_char;
                Object
            );
            let mut value: Object = ptr::null_mut();
            let found = send!(
                url,
                c"getResourceValue:forKey:error:",
                &mut value => *mut Object,
                key => Object,
                ptr::null_mut::<Object>() => *mut Object;
                ObjcBool
            );
            if found == ObjcBool::default() || value.is_null() {
                return None;
            }
            u64::try_from(send!(value, c"longLongValue"; i64))
                .ok()
                .filter(|&bytes| bytes > 0)
        })
    }

    /// Local snapshot names from `tmutil listlocalsnapshots`. Listing them with
    /// `fs_snapshot_list` needs root.
    pub fn snapshots(mount: &Path) -> Option<Vec<String>> {
        let output = Command::new("/usr/bin/tmutil")
            .arg("listlocalsnapshots")
            .arg(mount)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        output
            .status
            .success()
            .then(|| super::parse_snapshots(&output.stdout))
    }

    /// The APFS container of a volume's device, as `disk3` for `/dev/disk3s5s1`.
    pub(super) fn container(device: &str) -> Option<&str> {
        let name = device.strip_prefix("/dev/")?;
        let digits = name.strip_prefix("disk")?;
        let end = digits
            .find(|character: char| !character.is_ascii_digit())
            .unwrap_or(digits.len());
        (end > 0).then(|| &name[..4 + end])
    }

    /// The mounted volumes of `device`'s APFS container with each one's used space.
    pub fn container_volumes(device: &str) -> io::Result<Vec<(PathBuf, u64)>> {
        let Some(wanted) = container(device) else {
            return Ok(Vec::new());
        };
        let mut devices = Vec::new();
        let mut list = Vec::new();
        for mount in volumes::mounts()? {
            if mount.kind != "apfs"
                || container(&mount.device) != Some(wanted)
                || devices.contains(&mount.device)
            {
                continue;
            }
            if let Some(used) = space_used(&mount.path) {
                devices.push(mount.device);
                list.push((mount.path, used));
            }
        }
        list.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        Ok(list)
    }

    /// A volume's own used space, `ATTR_VOL_SPACEUSED`, which `statfs` doesn't report on
    /// APFS: it gives the whole container's.
    fn space_used(path: &Path) -> Option<u64> {
        let path = CString::new(path.as_os_str().as_bytes()).ok()?;
        let mut request = libc::attrlist {
            bitmapcount: libc::ATTR_BIT_MAP_COUNT,
            reserved: 0,
            commonattr: 0,
            volattr: libc::ATTR_VOL_INFO | libc::ATTR_VOL_SPACEUSED,
            dirattr: 0,
            fileattr: 0,
            forkattr: 0,
        };
        // A u32 length, then the off_t.
        let mut buffer = [0u8; 16];
        // SAFETY: the request and buffer outlive the call, which writes at most
        // `buffer.len()` bytes.
        let status = unsafe {
            libc::getattrlist(
                path.as_ptr(),
                (&raw mut request).cast(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                0,
            )
        };
        if status != 0 {
            return None;
        }
        let used = i64::from_ne_bytes(buffer[4..12].try_into().ok()?);
        u64::try_from(used).ok()
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::collections::HashSet;
    use std::ffi::OsStr;
    use std::fs;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::MetadataExt;
    use std::path::PathBuf;

    use super::OpenFile;

    const DELETED: &[u8] = b" (deleted)";

    /// Deleted regular files on `device` that processes still hold open, each counted
    /// once, and the number of processes whose files this user can't see.
    pub fn open_deleted(device: u64) -> (Vec<OpenFile>, usize) {
        let Ok(processes) = fs::read_dir("/proc") else {
            return (Vec::new(), 0);
        };
        let mut seen = HashSet::new();
        let mut files = Vec::new();
        let mut hidden = 0;
        for process in processes.flatten() {
            let Some(pid) = process
                .file_name()
                .to_str()
                .and_then(|name| name.parse::<u32>().ok())
            else {
                continue;
            };
            let directory = process.path().join("fd");
            let Ok(descriptors) = fs::read_dir(&directory) else {
                // A process that exited is gone. One this user can't inspect is hidden.
                hidden += usize::from(directory.exists());
                continue;
            };
            let mut program = None;
            for descriptor in descriptors.flatten() {
                let Ok(target) = fs::read_link(descriptor.path()) else {
                    continue;
                };
                let Some(path) = target.as_os_str().as_bytes().strip_suffix(DELETED) else {
                    continue;
                };
                // The link resolves to the open file even after it was deleted.
                let Ok(metadata) = fs::metadata(descriptor.path()) else {
                    continue;
                };
                if !metadata.is_file()
                    || metadata.nlink() != 0
                    || metadata.dev() != device
                    || !seen.insert(metadata.ino())
                {
                    continue;
                }
                let program = program
                    .get_or_insert_with(|| {
                        fs::read(process.path().join("comm"))
                            .map(|name| String::from_utf8_lossy(name.trim_ascii()).into_owned())
                            .unwrap_or_default()
                    })
                    .clone();
                files.push(OpenFile {
                    path: PathBuf::from(OsStr::from_bytes(path)),
                    allocated: metadata.blocks().saturating_mul(512),
                    apparent: metadata.size(),
                    pid,
                    program,
                });
            }
        }
        files.sort_by(|a, b| b.allocated.cmp(&a.allocated).then(a.pid.cmp(&b.pid)));
        (files, hidden)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn explanation() -> Explanation {
        Explanation {
            volume: Volume {
                path: "/".into(),
                device: "/dev/sda1".into(),
                kind: "ext4".into(),
                total_bytes: 100_000_000_000,
                used_bytes: 60_000_000_000,
                available_bytes: 35_000_000_000,
                total_inodes: 0,
                free_inodes: 0,
                read_only: false,
            },
            reserved: 5_000_000_000,
            purgeable: Some(10_390_000_000),
            snapshots: Some(vec![
                "com.apple.TimeMachine.2026-11-20-101500.local".into(),
                "com.apple.TimeMachine.2026-11-21-090000.local".into(),
            ]),
            container: vec![
                ("/System/Volumes/Data".into(), 813_500_000_000),
                ("/".into(), 13_760_000_000),
            ],
            open_files: vec![OpenFile {
                path: "/var/log/app.log".into(),
                allocated: 1_800_000_000,
                apparent: 1_799_999_000,
                pid: 1234,
                program: "java".into(),
            }],
            hidden_processes: 3,
        }
    }

    #[test]
    fn reads_snapshot_names_and_dates() {
        let output = b"Snapshots for disk /:\ncom.apple.TimeMachine.2026-11-21-101500.local\n\n";
        assert_eq!(
            parse_snapshots(output),
            ["com.apple.TimeMachine.2026-11-21-101500.local"]
        );
        assert!(parse_snapshots(b"Snapshots for disk /:\n").is_empty());
        assert_eq!(
            snapshot_date("com.apple.TimeMachine.2026-11-21-101500.local").as_deref(),
            Some("2026-11-21 10:15")
        );
        for other in [
            "com.apple.os.update-ABC",
            "com.apple.TimeMachine.2026-11-21.local",
            "com.apple.TimeMachine.2026-11-21-1015xx.local",
        ] {
            assert_eq!(snapshot_date(other), None, "{other}");
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn finds_the_apfs_container_of_a_device() {
        assert_eq!(macos::container("/dev/disk3s5"), Some("disk3"));
        assert_eq!(macos::container("/dev/disk3s1s1"), Some("disk3"));
        assert_eq!(macos::container("/dev/disk12s2"), Some("disk12"));
        assert_eq!(macos::container("map auto_home"), None);
        assert_eq!(macos::container("/dev/diskX"), None);
    }

    #[test]
    fn prints_the_explanation() {
        let mut plain = Vec::new();
        write_plain(&mut plain, &explanation()).unwrap();
        let plain = String::from_utf8(plain).unwrap();
        assert!(plain.starts_with("Volume      / (ext4, /dev/sda1)\nSize        100.00 GB\n"));
        assert!(plain.contains("Reserved    5.00 GB for root"));
        assert!(plain.contains("Purgeable   10.39 GB of caches"));
        if cfg!(target_os = "macos") {
            assert!(
                plain.contains(
                    "Snapshots   2 local, 2026-11-20 10:15 to 2026-11-21 09:00, holding deleted files' space\n"
                )
            );
            assert!(plain.contains("Used        60.00 GB, shared by the volumes"));
            assert!(plain.contains(" 813.50 GB  /System/Volumes/Data\n"));
        } else {
            assert!(
                plain.contains(
                    "Open files  1.80 GB in 1 deleted file that programs still hold open"
                )
            );
            assert!(plain.contains("   1.80 GB      1234  java             /var/log/app.log\n"));
        }

        let mut json = Vec::new();
        write_json(&mut json, &explanation()).unwrap();
        let json: serde_json::Value = serde_json::from_slice(&json).unwrap();
        assert_eq!(json["schemaVersion"], 3);
        assert_eq!(json["volume"]["path"], "/");
        assert_eq!(json["volume"]["type"], "ext4");
        assert_eq!(json["reservedBytes"], 5_000_000_000u64);
        assert_eq!(json["purgeableBytes"], 10_390_000_000u64);
        assert_eq!(json["snapshots"][1]["date"], "2026-11-21 09:00");
        assert_eq!(json["containerVolumes"][0]["usedBytes"], 813_500_000_000u64);
        assert_eq!(json["deletedOpenFiles"][0]["program"], "java");
        assert_eq!(json["hiddenProcesses"], 3);
    }

    #[test]
    fn explains_the_volume_of_a_folder() {
        let explanation = explain(Path::new(".")).unwrap();
        assert!(explanation.volume.total_bytes > 0);
        #[cfg(target_os = "macos")]
        assert!(
            explanation
                .container
                .iter()
                .any(|(path, _)| *path == explanation.volume.path)
                || explanation.volume.kind != "apfs"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn finds_a_deleted_file_this_process_holds_open() {
        use std::os::unix::fs::MetadataExt;
        let directory =
            std::env::temp_dir().join(format!("space16x-explain-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("held.log");
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(&[7; 64 * 1024]).unwrap();
        file.sync_all().unwrap();
        let device = std::fs::metadata(&path).unwrap().dev();
        std::fs::remove_file(&path).unwrap();
        let (files, _) = linux::open_deleted(device);
        std::fs::remove_dir(&directory).unwrap();
        let held = files
            .iter()
            .find(|file| file.path == path)
            .expect("the held file is listed");
        assert_eq!(held.pid, std::process::id());
        assert_eq!(held.apparent, 64 * 1024);
        drop(file);
    }
}
