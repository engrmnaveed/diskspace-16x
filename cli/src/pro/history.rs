//! Scan history: `space16x history save` keeps a scan, `space16x history` lists the kept
//! scans, and `space16x history diff` compares a folder with one of them.
//!
//! Saved scans are ncdu JSON exports, the same as `scan --export`, in
//! `$XDG_STATE_HOME/space16x/history` (by default `~/.local/state/space16x/history`).
//! `index.jsonl` there lists each one with its totals, one JSON object per line, so a
//! listing doesn't read every scan. Nothing is written except by `history save`.

use std::ffi::{OsStr, OsString};
use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::modified::{format_time, utc_stamp};
use crate::report::{self, Metric, Selection, format_bytes, terminal_path};
use crate::{ExcludeOptions, ncdu, scanner};

/// The index of saved scans in the history folder.
const INDEX: &str = "index.jsonl";
const SECONDS_PER_DAY: u64 = 86_400;

/// What a `history` command does.
#[derive(Debug, PartialEq, Eq)]
pub enum HistoryCommand {
    /// Scan a folder and keep the scan.
    Save {
        path: PathBuf,
        excludes: ExcludeOptions,
        threads: Option<usize>,
    },
    /// List the kept scans, of one folder or all.
    List { path: Option<PathBuf>, json: bool },
    /// Compare a folder now with its newest kept scan at least `since` days old.
    Diff {
        path: PathBuf,
        since: u32,
        selection: Selection,
        metric: Metric,
        json: bool,
        threads: Option<usize>,
        budget: Option<u64>,
    },
}

/// One kept scan, as `index.jsonl` records it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Saved {
    /// The export's file name in the history folder.
    file: String,
    scope: String,
    /// The scope in hexadecimal, when it isn't UTF-8.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    scope_bytes: Option<String>,
    /// When the scan was saved, in Unix seconds.
    saved_at: u64,
    allocated_bytes: u64,
    apparent_bytes: u64,
    item_count: u64,
    is_partial: bool,
}

impl Saved {
    fn scope_path(&self) -> PathBuf {
        match self.scope_bytes.as_deref().and_then(from_hex) {
            Some(bytes) => PathBuf::from(OsString::from_vec(bytes)),
            None => PathBuf::from(&self.scope),
        }
    }
}

fn from_hex(hex: &str) -> Option<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(hex.get(index..index + 2)?, 16).ok())
        .collect()
}

/// The history folder: `$XDG_STATE_HOME/space16x/history`, or
/// `~/.local/state/space16x/history` when that isn't set to an absolute path.
pub fn directory() -> io::Result<PathBuf> {
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute());
    let state = match state {
        Some(state) => state,
        None => std::env::var_os("HOME")
            .filter(|home| !home.is_empty())
            .map(|home| Path::new(&home).join(".local/state"))
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "Set HOME or XDG_STATE_HOME to keep a scan history.",
                )
            })?,
    };
    Ok(state.join("space16x/history"))
}

pub fn run(command: HistoryCommand) -> io::Result<u8> {
    match command {
        HistoryCommand::Save {
            path,
            excludes,
            threads,
        } => save(&path, &excludes, threads),
        HistoryCommand::List { path, json } => list(path.as_deref(), json),
        HistoryCommand::Diff {
            path,
            since,
            selection,
            metric,
            json,
            threads,
            budget,
        } => {
            let folder = directory()?;
            let scope = scanner::scope(&path)?;
            let cutoff = now().saturating_sub(u64::from(since) * SECONDS_PER_DAY);
            let saved = read_index(&folder)?
                .into_iter()
                .filter(|saved| saved.scope_path() == scope && saved.saved_at <= cutoff)
                .max_by_key(|saved| saved.saved_at)
                .ok_or_else(|| {
                    let older = match since {
                        0 => String::new(),
                        1 => " from at least 1 day ago".into(),
                        days => format!(" from at least {days} days ago"),
                    };
                    io::Error::new(
                        io::ErrorKind::NotFound,
                        format!(
                            "No saved scan of {}{older}. Save one with space16x history save.",
                            terminal_path(&scope)
                        ),
                    )
                })?;
            if !json {
                eprintln!(
                    "space16x: comparing with the scan saved {}",
                    format_time(saved.saved_at)
                );
            }
            super::run_diff(super::DiffOptions {
                old: folder.join(&saved.file),
                new: scope,
                selection,
                metric,
                json,
                threads,
                budget,
            })
        }
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// Scans `path`, writes the export to the history folder and adds it to the index.
fn save(path: &Path, excludes: &ExcludeOptions, threads: Option<usize>) -> io::Result<u8> {
    let rules = excludes.load()?;
    let scan = scanner::scan(
        path,
        scanner::Options {
            excludes: &rules,
            threads: threads.unwrap_or_else(scanner::default_threads),
        },
        false,
        &AtomicBool::new(false),
        |_| {},
    )
    .map_err(|error| crate::scan_error(path, error))?;
    let folder = directory()?;
    let saved = keep(&folder, &scan, rules.sources(), now())
        .map_err(|error| crate::scan_error(&folder, error))?;
    let root = scan.root();
    println!(
        "Saved {} ({}, {} items) to {}",
        terminal_path(scan.tree.root_path()),
        format_bytes(root.allocated_bytes()),
        root.item_count(),
        terminal_path(&folder.join(&saved.file))
    );
    crate::report_issues(&scan.issues, true);
    crate::report_excluded(scan.excluded_count, rules.sources().len());
    Ok(crate::status(&scan))
}

/// Writes `scan` to `folder`, readable only by this user, and records it in the index.
fn keep(
    folder: &Path,
    scan: &scanner::Report,
    excludes: &[String],
    time: u64,
) -> io::Result<Saved> {
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(folder)?;
    let scope = scan.tree.root_path();
    let name = file_name(scope, time);
    // Write under a hidden name first, so an interrupted save leaves no partial scan.
    let mut attempt = 1;
    let (file, partial) = loop {
        let file = if attempt == 1 {
            format!("{name}.json")
        } else {
            format!("{name}-{attempt}.json")
        };
        let partial = folder.join(format!(".{file}.partial"));
        if !folder.join(&file).exists() {
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&partial)
            {
                Ok(output) => {
                    let mut output = BufWriter::new(output);
                    ncdu::write(&mut |bytes| output.write_all(bytes), scan, excludes)?;
                    output
                        .into_inner()
                        .map_err(|error| error.into_error())?
                        .sync_all()?;
                    break (file, partial);
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        }
        attempt += 1;
    };
    fs::rename(&partial, folder.join(&file))?;
    let json = report::JsonPath::from(scope);
    let root = scan.root();
    let saved = Saved {
        file,
        scope: json.path.into_owned(),
        scope_bytes: json.path_bytes,
        saved_at: time,
        allocated_bytes: root.allocated_bytes(),
        apparent_bytes: root.apparent_bytes(),
        item_count: root.item_count(),
        is_partial: !scan.issues.is_empty(),
    };
    let mut line = serde_json::to_string(&saved)?;
    line.push('\n');
    OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o600)
        .open(folder.join(INDEX))?
        .write_all(line.as_bytes())?;
    Ok(saved)
}

/// A file name for a scan of `scope` saved at `time`: the UTC time, then the folder's
/// name in safe characters, as in `20261121T161500Z-srv`.
fn file_name(scope: &Path, time: u64) -> String {
    let base = scope.file_name().unwrap_or(OsStr::new("root"));
    let mut name = utc_stamp(time);
    name.push('-');
    let mut safe = String::new();
    for &byte in base.as_bytes().iter().take(48) {
        let character = byte as char;
        safe.push(
            if character.is_ascii_alphanumeric() || "._-".contains(character) {
                character
            } else {
                '_'
            },
        );
    }
    name.push_str(if safe.trim_matches('.').is_empty() {
        "root"
    } else {
        safe.trim_start_matches('.')
    });
    name
}

/// The kept scans whose files still exist, oldest first. Lines that can't be read are
/// skipped, so one damaged line doesn't hide the rest.
fn read_index(folder: &Path) -> io::Result<Vec<Saved>> {
    let index = match File::open(folder.join(INDEX)) {
        Ok(index) => index,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(crate::scan_error(&folder.join(INDEX), error)),
    };
    let mut saved = Vec::new();
    for line in BufReader::new(index).lines() {
        let Ok(entry) = serde_json::from_str::<Saved>(&line?) else {
            continue;
        };
        let safe = !entry.file.contains('/') && !entry.file.starts_with('.');
        if safe && folder.join(&entry.file).is_file() {
            saved.push(entry);
        }
    }
    Ok(saved)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonList<'a> {
    schema_version: u8,
    edition: &'static str,
    directory: report::JsonPath<'a>,
    scans: Vec<JsonSaved<'a>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonSaved<'a> {
    #[serde(flatten)]
    saved: &'a Saved,
    /// The export's full path, which `diff` and `--import` open.
    path: report::JsonPath<'static>,
}

/// Lists the kept scans, newest first, of `path` or of every folder.
fn list(path: Option<&Path>, json: bool) -> io::Result<u8> {
    let folder = directory()?;
    let scope = path.map(scanner::scope).transpose()?;
    let mut saved: Vec<Saved> = read_index(&folder)?
        .into_iter()
        .filter(|saved| {
            scope
                .as_ref()
                .is_none_or(|scope| saved.scope_path() == *scope)
        })
        .collect();
    saved.sort_by_key(|saved| std::cmp::Reverse(saved.saved_at));
    let mut out = BufWriter::new(io::stdout().lock());
    if json {
        let scans = saved
            .iter()
            .map(|saved| {
                let file = folder.join(&saved.file);
                let report::JsonPath { path, path_bytes } = report::JsonPath::from(file.as_path());
                JsonSaved {
                    saved,
                    path: report::JsonPath {
                        path: path.into_owned().into(),
                        path_bytes,
                    },
                }
            })
            .collect();
        serde_json::to_writer_pretty(
            &mut out,
            &JsonList {
                schema_version: 3,
                edition: report::EDITION,
                directory: report::JsonPath::from(folder.as_path()),
                scans,
            },
        )?;
        writeln!(out)?;
        return out.flush().map(|()| 0);
    }
    if saved.is_empty() {
        let of = scope.map_or_else(String::new, |scope| {
            format!(" of {}", terminal_path(&scope))
        });
        eprintln!("space16x: no saved scans{of} yet. Save one with space16x history save PATH.");
        return Ok(0);
    }
    writeln!(out, "Saved                   Size      Items  Path")?;
    for saved in &saved {
        let mut line = format!(
            "{:<16}  {:>10}  {:>9}  {}",
            format_time(saved.saved_at),
            format_bytes(saved.allocated_bytes),
            saved.item_count,
            terminal_path(&saved.scope_path())
        );
        if saved.is_partial {
            line.push_str("  [partial]");
        }
        writeln!(out, "{line}")?;
    }
    out.flush()?;
    eprintln!("space16x: saved scans are in {}", terminal_path(&folder));
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::{EntryState, TreeBuilder};
    use std::sync::Arc;

    fn scan(root: &str, bytes: u64) -> scanner::Report {
        let mut builder = TreeBuilder::new(root.into(), EntryState::Local);
        builder.file(b"data", bytes, bytes, EntryState::Local);
        scanner::Report {
            tree: Arc::new(builder.finish().0),
            issues: Vec::new(),
            excluded_count: 0,
            is_preview: false,
        }
    }

    #[test]
    fn keeps_scans_and_indexes_them() {
        let folder = std::env::temp_dir().join(format!("space16x-history-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        let first = keep(
            &folder,
            &scan("/srv", 100),
            &["*.log".into()],
            1_800_000_000,
        )
        .unwrap();
        let second = keep(&folder, &scan("/srv", 300), &[], 1_800_000_000).unwrap();
        let other = keep(&folder, &scan("/", 7), &[], 1_800_000_100).unwrap();
        assert_eq!(first.file, "20270115T080000Z-srv.json");
        assert_eq!(second.file, "20270115T080000Z-srv-2.json");
        assert_eq!(other.file, "20270115T080140Z-root.json");
        assert!(!folder.read_dir().unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".partial")
        }));
        let mode = fs::metadata(&folder).unwrap().permissions();
        assert_eq!(
            std::os::unix::fs::PermissionsExt::mode(&mode) & 0o777,
            0o700
        );

        // A damaged line and a missing file are skipped.
        fs::OpenOptions::new()
            .append(true)
            .open(folder.join(INDEX))
            .unwrap()
            .write_all(b"not json\n")
            .unwrap();
        fs::remove_file(folder.join(&second.file)).unwrap();
        let index = read_index(&folder).unwrap();
        assert_eq!(index, [first.clone(), other]);
        assert_eq!(index[0].scope_path(), Path::new("/srv"));
        assert_eq!(index[0].allocated_bytes, 100);

        let saved = File::open(folder.join(&first.file)).unwrap();
        let mut saved = BufReader::new(saved);
        let imported = ncdu::read(&mut |buffer| io::Read::read(&mut saved, buffer)).unwrap();
        assert_eq!(imported.excludes, ["*.log"]);
        assert_eq!(imported.report.root().allocated_bytes(), 100);
        fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn names_files_after_the_time_and_folder() {
        assert_eq!(
            file_name(Path::new("/Users/me/My Files"), 0),
            "19700101T000000Z-My_Files"
        );
        assert_eq!(
            file_name(Path::new("/srv/.cache"), 0),
            "19700101T000000Z-cache"
        );
        assert_eq!(file_name(Path::new("/srv/.."), 0), "19700101T000000Z-root");
        assert_eq!(file_name(Path::new("/"), 0), "19700101T000000Z-root");
        // Keep in sync with ScanHistoryTests in the Swift package.
        assert_eq!(
            file_name(Path::new("/Volumes/Café"), 0),
            "19700101T000000Z-Caf__"
        );
        let long = "x".repeat(100);
        assert_eq!(file_name(Path::new(&long), 0).len(), 17 + 48);
        assert_eq!(from_hex("2f6aff"), Some(vec![0x2f, 0x6a, 0xff]));
        assert_eq!(from_hex("2f6"), None);
    }
}
