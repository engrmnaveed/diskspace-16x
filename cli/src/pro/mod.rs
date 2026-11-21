//! Commands of the Pro edition, built with `cargo build --features pro`.
//!
//! Basic builds leave this module out. `main.rs` lists the Pro subcommands, answers them
//! with an install hint in Basic, and hands them to [`parse`] and [`run`] here. Per-node
//! Pro data belongs in side tables keyed by the scan tree's `NodeId`, so Basic's tree stays
//! lean.

mod cleanup;
mod diff;
mod exclusive;
mod explain;
mod history;
mod modified;
mod owners;
mod types;

pub use cleanup::{rule_for as cleanup_rule, suggestions as cleanup_suggestions};
pub use modified::format_date;
pub use types::{TypeTotal, breakdown as type_breakdown};

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::SystemTime;

use crate::report::{self, Metric, Selection};
use crate::scanner;

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Diff(DiffOptions),
    History(history::HistoryCommand),
    Explain { path: PathBuf, json: bool },
}

#[derive(Debug, PartialEq, Eq)]
pub struct DiffOptions {
    old: PathBuf,
    new: PathBuf,
    selection: Selection,
    metric: Metric,
    json: bool,
    threads: Option<usize>,
}

/// Options only Pro's `scan` takes, listed in `main.rs`'s `PRO_SCAN_OPTIONS`.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ScanOptions {
    /// Count only files last modified more than this many days ago.
    older_than: Option<u32>,
    /// List the N largest file types instead of entries.
    types: Option<usize>,
    /// List each owner's bytes and items instead of entries.
    by_owner: bool,
    /// List cleanup suggestions instead of entries.
    cleanup: bool,
    /// Add each entry's exclusive bytes, which removing it frees at least.
    exclusive: bool,
}

impl ScanOptions {
    /// Takes the value after one of the Pro options.
    pub fn take(
        &mut self,
        flag: &str,
        arguments: &mut impl Iterator<Item = OsString>,
    ) -> Result<(), String> {
        match flag {
            "--older-than" if self.older_than.is_none() => {
                let days = arguments
                    .next()
                    .and_then(|value| value.to_str()?.parse::<u32>().ok())
                    .ok_or("--older-than requires a number of days.")?;
                self.older_than = Some(days);
                Ok(())
            }
            "--types" if self.types.is_none() => {
                let limit = arguments
                    .next()
                    .and_then(|value| value.to_str()?.parse::<usize>().ok())
                    .ok_or("--types requires a nonnegative integer.")?;
                self.types = Some(limit);
                Ok(())
            }
            "--by-owner" if !self.by_owner => {
                self.by_owner = true;
                Ok(())
            }
            "--cleanup" if !self.cleanup => {
                self.cleanup = true;
                Ok(())
            }
            "--exclusive" if !self.exclusive => {
                self.exclusive = true;
                Ok(())
            }
            _ => Err(format!("Use {flag} once.")),
        }
    }

    /// Checks the options against the rest of the scan command.
    pub fn check(
        &self,
        selection: Option<Selection>,
        metric: Option<Metric>,
    ) -> Result<(), String> {
        if self.exclusive && (self.types.is_some() || self.by_owner || self.cleanup) {
            return Err("--exclusive adds a column to entries or --top, so it takes no --types, --by-owner or --cleanup.".into());
        }
        if self.exclusive && metric.is_some() {
            return Err("--exclusive measures allocated bytes, so it takes no --apparent.".into());
        }
        let reports = [
            selection.is_some(),
            self.types.is_some(),
            self.by_owner,
            self.cleanup,
        ];
        if reports.into_iter().filter(|&chosen| chosen).count() > 1 {
            return Err("Use one of --depth, --top, --types, --by-owner and --cleanup.".into());
        }
        Ok(())
    }

    /// Writes a Pro report, such as `--types`, to standard output. Returns false when the
    /// options ask for the usual report.
    pub fn write_report(
        &self,
        report: &scanner::Report,
        selection: Selection,
        metric: Metric,
        excludes: &[String],
        json: bool,
        threads: usize,
    ) -> io::Result<bool> {
        let stdout = io::stdout().lock();
        if self.exclusive {
            let measured = exclusive::Exclusive::measure(&report.tree, threads);
            if json {
                let result = exclusive::JsonExclusive::new(report, selection, &measured);
                report::write_json_report(
                    stdout,
                    report,
                    metric,
                    excludes,
                    self.older_than,
                    result,
                )?;
            } else {
                exclusive::write_plain(stdout, report, selection, &measured)?;
                eprintln!(
                    "space16x: exclusive sizes leave out blocks shared with clones and files with other hard links, which emptying the Trash wouldn't free"
                );
            }
            return Ok(true);
        }
        if self.cleanup {
            if json {
                let result = cleanup::JsonCleanup::new(report, metric);
                report::write_json_report(
                    stdout,
                    report,
                    metric,
                    excludes,
                    self.older_than,
                    result,
                )?;
            } else {
                cleanup::write_plain(stdout, report, metric)?;
                eprintln!(
                    "space16x: tools recreate these folders when needed. Review them, then press u in the explorer to move them to the Trash."
                );
            }
            return Ok(true);
        }
        match (self.types, self.by_owner, json) {
            (Some(limit), _, true) => {
                let result = types::JsonTypes::new(report, limit, metric);
                report::write_json_report(
                    stdout,
                    report,
                    metric,
                    excludes,
                    self.older_than,
                    result,
                )?;
            }
            (Some(limit), _, false) => types::write_plain(stdout, report, limit, metric)?,
            (None, true, true) => {
                let result = owners::JsonOwners::new(report, metric);
                report::write_json_report(
                    stdout,
                    report,
                    metric,
                    excludes,
                    self.older_than,
                    result,
                )?;
            }
            (None, true, false) => owners::write_plain(stdout, report, metric)?,
            (None, false, _) => return Ok(false),
        }
        Ok(true)
    }

    pub fn older_than_days(&self) -> Option<u32> {
        self.older_than
    }

    /// Applies the options to a finished scan: `--older-than` keeps only older files.
    pub fn narrow(&self, report: scanner::Report) -> scanner::Report {
        match self.older_than {
            Some(days) => modified::older_than(report, modified::cutoff(days, SystemTime::now())),
            None => report,
        }
    }

    /// Notes on standard error how the options narrowed a text report.
    pub fn report_narrowed(&self) {
        if let Some(days) = self.older_than {
            let before = modified::cutoff(days, SystemTime::now());
            eprintln!(
                "space16x: counted only files last modified more than {days} {} ago, before {}",
                if days == 1 { "day" } else { "days" },
                format_date(before)
            );
        }
    }
}

pub fn parse(
    name: &str,
    arguments: impl Iterator<Item = OsString>,
) -> Result<crate::Command, String> {
    match name {
        "diff" => parse_diff(arguments),
        "history" => parse_history(arguments),
        "explain" => parse_explain(arguments),
        _ => Err(format!("Unknown Pro command {name:?}.")),
    }
}

/// Parses `explain [PATH] [--json]`.
fn parse_explain(arguments: impl Iterator<Item = OsString>) -> Result<crate::Command, String> {
    let mut path = None;
    let mut json = false;
    let mut literal = false;
    for argument in arguments {
        if literal || !crate::is_option(&argument) {
            crate::set_path(&mut path, crate::checked_path(argument)?)?;
            continue;
        }
        match argument.to_str() {
            Some("-h" | "--help") if path.is_none() && !json => return Ok(crate::Command::Help),
            Some("--") => literal = true,
            Some("--json") if !json => json = true,
            _ => return Err(format!("Unknown or repeated option {argument:?}.")),
        }
    }
    Ok(crate::Command::Pro(Command::Explain {
        path: path.unwrap_or_else(|| PathBuf::from(".")),
        json,
    }))
}

/// Parses `history [PATH]`, `history save [PATH]` and `history diff [PATH]`.
fn parse_history(arguments: impl Iterator<Item = OsString>) -> Result<crate::Command, String> {
    let mut arguments = arguments.peekable();
    let action = match arguments.peek().and_then(|argument| argument.to_str()) {
        Some(action @ ("save" | "list" | "diff")) => {
            let action = action.to_owned();
            arguments.next();
            action
        }
        _ => "list".to_owned(),
    };
    let mut path = None;
    let mut excludes = crate::ExcludeOptions::default();
    let mut threads = None;
    let mut json = false;
    let mut since = None;
    let mut selection = None;
    let mut metric = None;
    let mut literal = false;
    let mut chosen = 0;
    while let Some(argument) = arguments.next() {
        if literal || !crate::is_option(&argument) {
            crate::set_path(&mut path, crate::checked_path(argument)?)?;
            continue;
        }
        chosen += 1;
        match (action.as_str(), argument.to_str()) {
            (_, Some("-h" | "--help")) if chosen == 1 && path.is_none() => {
                return Ok(crate::Command::Help);
            }
            (_, Some("--")) => literal = true,
            ("save", Some(flag @ ("--exclude" | "--exclude-from"))) => {
                excludes.take(flag, &mut arguments)?
            }
            ("save" | "diff", Some("--threads")) => {
                crate::take_threads(&mut threads, &mut arguments)?
            }
            ("list" | "diff", Some("--json")) if !json => json = true,
            ("diff", Some("--since")) if since.is_none() => {
                let days = arguments
                    .next()
                    .and_then(|value| value.to_str()?.parse::<u32>().ok())
                    .ok_or("--since requires a number of days.")?;
                since = Some(days);
            }
            ("diff", Some(flag @ ("--depth" | "--top"))) if selection.is_none() => {
                let number = arguments
                    .next()
                    .and_then(|value| value.to_str()?.parse::<usize>().ok())
                    .ok_or_else(|| format!("{flag} requires a nonnegative integer."))?;
                selection = Some(if flag == "--depth" {
                    Selection::Depth(number)
                } else {
                    Selection::Top(number)
                });
            }
            ("diff", Some("--apparent")) if metric.is_none() => metric = Some(Metric::Apparent),
            _ => return Err(format!("Unknown or repeated option {argument:?}.")),
        }
    }
    let command = match action.as_str() {
        "save" => history::HistoryCommand::Save {
            path: path.unwrap_or_else(|| PathBuf::from(".")),
            excludes,
            threads,
        },
        "diff" => history::HistoryCommand::Diff {
            path: path.unwrap_or_else(|| PathBuf::from(".")),
            since: since.unwrap_or(0),
            selection: selection.unwrap_or(Selection::Depth(1)),
            metric: metric.unwrap_or_default(),
            json,
            threads,
        },
        _ => history::HistoryCommand::List { path, json },
    };
    Ok(crate::Command::Pro(Command::History(command)))
}

fn parse_diff(mut arguments: impl Iterator<Item = OsString>) -> Result<crate::Command, String> {
    let mut files = Vec::new();
    let mut selection = None;
    let mut metric = None;
    let mut json = false;
    let mut threads = None;
    let mut literal = false;
    while let Some(argument) = arguments.next() {
        if literal || argument == "-" || !crate::is_option(&argument) {
            files.push(crate::checked_path(argument)?);
            continue;
        }
        match argument.to_str() {
            Some("-h" | "--help") if files.is_empty() => return Ok(crate::Command::Help),
            Some("--") => literal = true,
            Some(flag @ ("--depth" | "--top")) if selection.is_none() => {
                let number = arguments
                    .next()
                    .and_then(|value| value.to_str()?.parse::<usize>().ok())
                    .ok_or_else(|| format!("{flag} requires a nonnegative integer."))?;
                selection = Some(if flag == "--depth" {
                    Selection::Depth(number)
                } else {
                    Selection::Top(number)
                });
            }
            Some("--json") if !json => json = true,
            Some("--apparent") if metric.is_none() => metric = Some(Metric::Apparent),
            Some("--threads") => crate::take_threads(&mut threads, &mut arguments)?,
            _ => return Err(format!("Unknown or repeated option {argument:?}.")),
        }
    }
    let [old, new]: [PathBuf; 2] = files
        .try_into()
        .map_err(|_| "diff compares two scans: space16x diff OLD NEW|PATH.")?;
    if old == Path::new("-") && new == Path::new("-") {
        return Err("Only one scan can come from standard input.".into());
    }
    Ok(crate::Command::Pro(Command::Diff(DiffOptions {
        old,
        new,
        selection: selection.unwrap_or(Selection::Depth(1)),
        metric: metric.unwrap_or_default(),
        json,
        threads,
    })))
}

pub fn run(command: Command) -> io::Result<u8> {
    match command {
        Command::Diff(options) => run_diff(options),
        Command::History(command) => history::run(command),
        Command::Explain { path, json } => explain::run(&path, json),
    }
}

/// Compares a saved scan with another saved scan, or with a folder scanned now using the
/// saved scan's exclude patterns.
fn run_diff(options: DiffOptions) -> io::Result<u8> {
    let old = crate::import(&options.old)?;
    let is_folder = options.new != Path::new("-")
        && std::fs::metadata(&options.new).is_ok_and(|metadata| metadata.is_dir());
    let (new, excludes) = if is_folder {
        let excludes = scanner::Excludes::new(&old.excludes)
            .map_err(|message| io::Error::new(io::ErrorKind::InvalidData, message))?;
        let report = scanner::scan(
            &options.new,
            scanner::Options {
                excludes: &excludes,
                threads: options.threads.unwrap_or_else(scanner::default_threads),
            },
            false,
            &AtomicBool::new(false),
            |_| {},
        )
        .map_err(|error| crate::scan_error(&options.new, error))?;
        (report, Some(excludes))
    } else {
        (crate::import(&options.new)?.report, None)
    };
    let summary = diff::Summary {
        old: &old.report,
        new: &new,
        metric: options.metric,
    };
    let changes = match options.selection {
        Selection::Depth(depth) => {
            diff::entries(&old.report.tree, &new.tree, depth, options.metric)
        }
        Selection::Top(limit) => diff::files(&old.report.tree, &new.tree, limit, options.metric),
    };
    let stdout = io::stdout().lock();
    if options.json {
        diff::write_json(stdout, &summary, options.selection, &changes)?;
    } else {
        diff::write_plain(stdout, &summary, &changes)?;
        if let Some(excludes) = &excludes {
            crate::report_issues(&new.issues, true);
            crate::report_excluded(new.excluded_count, excludes.sources().len());
        }
    }
    let partial = !old.report.issues.is_empty() || !new.issues.is_empty();
    Ok(if partial { 2 } else { 0 })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Result<crate::Command, String> {
        parse("diff", values.iter().map(OsString::from))
    }

    #[test]
    fn parses_history_actions() {
        let history = |values: &[&str]| parse("history", values.iter().map(OsString::from));
        let command = |command| Ok(crate::Command::Pro(Command::History(command)));
        assert_eq!(
            history(&[]),
            command(history::HistoryCommand::List {
                path: None,
                json: false
            })
        );
        assert_eq!(
            history(&["/srv", "--json"]),
            command(history::HistoryCommand::List {
                path: Some("/srv".into()),
                json: true
            })
        );
        assert!(matches!(
            history(&["save", "--exclude", "*.log", "--threads", "2"]),
            Ok(crate::Command::Pro(Command::History(history::HistoryCommand::Save {
                ref path, threads: Some(2), ..
            }))) if path == Path::new(".")
        ));
        assert_eq!(
            history(&["diff", "/srv", "--since", "7", "--top", "5", "--json"]),
            command(history::HistoryCommand::Diff {
                path: "/srv".into(),
                since: 7,
                selection: Selection::Top(5),
                metric: Metric::Allocated,
                json: true,
                threads: None,
            })
        );
        // A folder named like an action can still be listed.
        assert!(matches!(
            history(&["./save"]),
            Ok(crate::Command::Pro(Command::History(
                history::HistoryCommand::List { path: Some(_), .. }
            )))
        ));
        assert_eq!(history(&["save", "--help"]), Ok(crate::Command::Help));
        for invalid in [
            &["save", "--json"][..],
            &["list", "--since", "2"],
            &["diff", "--since"],
            &["diff", "--depth", "1", "--top", "1"],
            &["a", "b"],
            &["list", "--exclude", "x"],
        ] {
            assert!(history(invalid).is_err(), "{invalid:?}");
        }
    }

    #[test]
    fn parses_explain() {
        let explain = |values: &[&str]| parse("explain", values.iter().map(OsString::from));
        assert_eq!(
            explain(&[]),
            Ok(crate::Command::Pro(Command::Explain {
                path: ".".into(),
                json: false
            }))
        );
        assert_eq!(
            explain(&["--json", "--", "-disk"]),
            Ok(crate::Command::Pro(Command::Explain {
                path: "-disk".into(),
                json: true
            }))
        );
        assert_eq!(explain(&["--help"]), Ok(crate::Command::Help));
        for invalid in [&["a", "b"][..], &["--json", "--json"], &["--all"]] {
            assert!(explain(invalid).is_err(), "{invalid:?}");
        }
    }

    #[test]
    fn takes_two_scans_and_report_options() {
        assert_eq!(
            args(&["old.json", "/data", "--top", "5", "--json", "--apparent"]),
            Ok(crate::Command::Pro(Command::Diff(DiffOptions {
                old: "old.json".into(),
                new: "/data".into(),
                selection: Selection::Top(5),
                metric: Metric::Apparent,
                json: true,
                threads: None,
            })))
        );
        assert!(matches!(
            args(&["-", "--threads", "2", "--", "-new"]),
            Ok(crate::Command::Pro(Command::Diff(DiffOptions { threads: Some(2), ref new, .. })))
                if new == Path::new("-new")
        ));
        assert_eq!(args(&["--help"]), Ok(crate::Command::Help));
        for invalid in [
            &["old.json"][..],
            &["a", "b", "c"],
            &["-", "-"],
            &["a", "b", "--depth", "1", "--top", "2"],
            &["a", "b", "--json", "--json"],
            &["a", "b", "--color"],
        ] {
            assert!(args(invalid).is_err(), "{invalid:?}");
        }
    }
}
