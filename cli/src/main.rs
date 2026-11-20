mod report;
mod scanner;
// The explorer starts moving marked items to the Trash in roadmap item M2.2.
#[allow(dead_code)]
mod trash;
mod tui;

use std::env;
use std::ffi::{OsStr, OsString};
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;

use report::{Metric, Selection};

const HELP: &str = "\
Usage: space16x [--no-color] [EXCLUDES] [PATH]
       space16x scan [PATH] [--depth N | --top N] [--apparent] [--json] [EXCLUDES]

Open the disk explorer in an interactive terminal. PATH defaults to the current directory.
The scan command prints a plain directory summary and works in scripts.

  --depth N  List entries through N levels (default: 1, 0 for the root total)
  --top N    List the N largest files in the scan instead of directory entries
  --apparent Show and rank apparent sizes (file lengths) instead of disk usage
  --json     Print a report in JSON schema 3
  --no-color Show the explorer without colors, as does setting NO_COLOR
  --         Treat the following argument as a path, even if it starts with a dash
  --version  Show the installed version
  -h, --help Show this help

EXCLUDES skip matching files and folders without reading them:
  --exclude PATTERN    Skip names or paths like node_modules, *.log, /build, cache/
  --exclude-from FILE  Read one pattern per line (# starts a comment)
  Patterns use * and ? within a name and ** across folders. A leading / anchors to
  PATH, and a trailing / matches folders only. Repeat options to add patterns.

Sizes are allocated bytes (disk usage) in decimal units (1 KB = 1,000 bytes), as in Finder.
Scan exit codes: 0 complete, 1 error, 2 partial.
";

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Help,
    Version,
    Explore(ExploreOptions),
    Scan(ScanOptions),
}

#[derive(Debug, PartialEq, Eq)]
struct ExploreOptions {
    path: PathBuf,
    excludes: ExcludeOptions,
    no_color: bool,
}

#[derive(Debug, PartialEq, Eq)]
struct ScanOptions {
    path: PathBuf,
    selection: Selection,
    metric: Metric,
    json: bool,
    excludes: ExcludeOptions,
}

/// Exclude patterns as given on the command line, loaded when the command runs.
#[derive(Debug, Default, PartialEq, Eq)]
struct ExcludeOptions {
    patterns: Vec<String>,
    files: Vec<PathBuf>,
}

impl ExcludeOptions {
    fn is_empty(&self) -> bool {
        self.patterns.is_empty() && self.files.is_empty()
    }

    /// Takes the value after `--exclude` or `--exclude-from`.
    fn take(
        &mut self,
        flag: &str,
        arguments: &mut impl Iterator<Item = OsString>,
    ) -> Result<(), String> {
        let value = arguments.next().filter(|value| !value.is_empty());
        if flag == "--exclude" {
            let pattern = value
                .and_then(|value| value.into_string().ok())
                .ok_or("--exclude requires a UTF-8 pattern.")?;
            self.patterns.push(pattern);
        } else {
            self.files
                .push(value.ok_or("--exclude-from requires a file path.")?.into());
        }
        Ok(())
    }

    fn load(&self) -> io::Result<scanner::Excludes> {
        let mut patterns = self.patterns.clone();
        for file in &self.files {
            let lines = scanner::Excludes::read_lines(file).map_err(|error| {
                io::Error::new(
                    error.kind(),
                    format!("{}: {error}", report::terminal_path(file)),
                )
            })?;
            patterns.extend(lines);
        }
        scanner::Excludes::new(patterns)
            .map_err(|message| io::Error::new(io::ErrorKind::InvalidInput, message))
    }
}

fn main() -> ExitCode {
    let command = match parse(env::args_os().skip(1)) {
        Ok(command) => command,
        Err(message) => {
            eprintln!("space16x: {message}\nRun 'space16x --help' for usage.");
            return ExitCode::FAILURE;
        }
    };
    match run(command) {
        Ok(status) => ExitCode::from(status),
        Err(error) if error.kind() == io::ErrorKind::Interrupted => ExitCode::from(130),
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("space16x: {}", report::terminal_text(&error.to_string()));
            ExitCode::FAILURE
        }
    }
}

fn run(command: Command) -> io::Result<u8> {
    match command {
        Command::Help => io::stdout().write_all(HELP.as_bytes())?,
        Command::Version => writeln!(io::stdout(), "space16x {}", env!("CARGO_PKG_VERSION"))?,
        Command::Explore(options)
            if io::stdin().is_terminal()
                && io::stdout().is_terminal()
                && env::var_os("TERM").is_none_or(|term| term != "dumb") =>
        {
            // https://no-color.org: any nonempty NO_COLOR value turns colors off.
            let plain = options.no_color || env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
            tui::run(&options.path, options.excludes.load()?, plain)?;
        }
        Command::Explore(options) => {
            return run_scan(ScanOptions {
                path: options.path,
                selection: Selection::Depth(1),
                metric: Metric::Allocated,
                json: false,
                excludes: options.excludes,
            });
        }
        Command::Scan(options) => return run_scan(options),
    }
    Ok(0)
}

fn run_scan(options: ScanOptions) -> io::Result<u8> {
    let excludes = options.excludes.load()?;
    let scan = scanner::scan(
        &options.path,
        &excludes,
        false,
        &AtomicBool::new(false),
        |_| {},
    )
    .map_err(|error| scan_error(&options.path, error))?;
    let stdout = io::stdout().lock();
    if options.json {
        let sources = excludes.sources();
        report::write_json(stdout, &scan, options.selection, options.metric, sources)?;
    } else {
        report::write_plain(stdout, &scan, options.selection, options.metric)?;
        report_issues(&scan.issues);
        report_excluded(scan.excluded_count, excludes.sources().len());
    }
    Ok(if scan.issues.is_empty() { 0 } else { 2 })
}

fn scan_error(path: &Path, error: io::Error) -> io::Error {
    io::Error::new(
        error.kind(),
        format!("{}: {error}", report::terminal_path(path)),
    )
}

/// Excluded paths are expected, so they get one summary line and leave the scan complete.
fn report_excluded(count: u64, patterns: usize) {
    if count > 0 {
        eprintln!(
            "space16x: excluded {count} {} matching {patterns} {}",
            if count == 1 { "path" } else { "paths" },
            if patterns == 1 { "pattern" } else { "patterns" }
        );
    }
}

/// Lists each scan error, then summarizes paths the OS refused in one line.
/// System locations hold hundreds of these, and JSON output still lists every one.
fn report_issues(issues: &[scanner::Issue]) {
    let mut protected = 0;
    let mut privacy_refusals = false;
    for issue in issues {
        if issue.kind.is_protected() {
            protected += 1;
            privacy_refusals |= issue.kind == scanner::IssueKind::NotPermitted;
            continue;
        }
        eprintln!(
            "space16x: {}: {}",
            report::terminal_path(&issue.path),
            report::terminal_text(&issue.message)
        );
    }
    if protected == 0 {
        return;
    }
    let os = if cfg!(target_os = "macos") {
        "macOS"
    } else {
        "the system"
    };
    eprintln!(
        "space16x: skipped {protected} protected {} (permission denied or blocked by {os}); use --json to list them",
        if protected == 1 { "path" } else { "paths" }
    );
    if privacy_refusals && !has_full_disk_access() {
        eprintln!(
            "space16x: give your terminal app Full Disk Access in System Settings to include privacy-protected folders"
        );
    }
}

/// macOS opens this folder only for processes with Full Disk Access. Opening it reads no contents.
#[cfg(target_os = "macos")]
fn has_full_disk_access() -> bool {
    match std::fs::File::open("/Library/Application Support/com.apple.TCC") {
        Ok(_) => true,
        Err(error) => error.raw_os_error() != Some(libc::EPERM),
    }
}

#[cfg(not(target_os = "macos"))]
fn has_full_disk_access() -> bool {
    true
}

fn parse(arguments: impl IntoIterator<Item = OsString>) -> Result<Command, String> {
    let mut arguments = arguments.into_iter().peekable();
    match arguments.peek().and_then(|argument| argument.to_str()) {
        Some("-h" | "--help") => return only_argument(arguments.count(), Command::Help),
        Some("--version") => return only_argument(arguments.count(), Command::Version),
        Some("scan") => {
            arguments.next();
            return parse_scan(arguments);
        }
        _ => {}
    }

    parse_explore(arguments)
}

fn parse_explore(mut arguments: impl Iterator<Item = OsString>) -> Result<Command, String> {
    let mut path = None;
    let mut excludes = ExcludeOptions::default();
    let mut no_color = false;
    while let Some(argument) = arguments.next() {
        match argument.to_str() {
            Some("--") => {
                set_path(&mut path, single_path(arguments, true)?)?;
                break;
            }
            Some(flag @ ("--exclude" | "--exclude-from")) => excludes.take(flag, &mut arguments)?,
            Some("--no-color") if !no_color => no_color = true,
            _ if is_option(&argument) => return Err(format!("Unknown option {argument:?}.")),
            _ => set_path(&mut path, checked_path(argument)?)?,
        }
    }
    Ok(Command::Explore(ExploreOptions {
        path: path.unwrap_or_else(|| PathBuf::from(".")),
        excludes,
        no_color,
    }))
}

fn only_argument(count: usize, command: Command) -> Result<Command, String> {
    if count == 1 {
        Ok(command)
    } else {
        Err("Unexpected arguments after the command.".into())
    }
}

fn single_path(
    mut arguments: impl Iterator<Item = OsString>,
    literal: bool,
) -> Result<PathBuf, String> {
    let Some(argument) = arguments.next() else {
        return if literal {
            Err("Provide a path after --.".into())
        } else {
            Ok(PathBuf::from("."))
        };
    };
    if !literal && is_option(&argument) {
        return Err(format!("Unknown option {argument:?}."));
    }
    if arguments.next().is_some() {
        return Err("Provide at most one path.".into());
    }
    checked_path(argument)
}

fn parse_scan(mut arguments: impl Iterator<Item = OsString>) -> Result<Command, String> {
    let mut path = None;
    let mut selection = None;
    let mut metric = None;
    let mut json = false;
    let mut excludes = ExcludeOptions::default();
    while let Some(argument) = arguments.next() {
        match argument.to_str() {
            Some("-h" | "--help")
                if path.is_none()
                    && selection.is_none()
                    && metric.is_none()
                    && !json
                    && excludes.is_empty() =>
            {
                return only_argument(arguments.count() + 1, Command::Help);
            }
            Some("--") => {
                let literal = single_path(arguments, true)?;
                set_path(&mut path, literal)?;
                break;
            }
            Some("--depth" | "--top") => {
                if selection.is_some() {
                    return Err("Use --depth or --top once per scan.".into());
                }
                let number = arguments
                    .next()
                    .and_then(|value| value.to_str()?.parse::<usize>().ok())
                    .ok_or_else(|| {
                        format!(
                            "{} requires a nonnegative integer.",
                            argument.to_string_lossy()
                        )
                    })?;
                selection = Some(if argument == "--depth" {
                    Selection::Depth(number)
                } else {
                    Selection::Top(number)
                });
            }
            Some("--json") if !json => json = true,
            Some("--apparent") if metric.is_none() => metric = Some(Metric::Apparent),
            Some(flag @ ("--exclude" | "--exclude-from")) => excludes.take(flag, &mut arguments)?,
            _ if is_option(&argument) => {
                return Err(format!("Unknown or repeated option {argument:?}."));
            }
            _ => set_path(&mut path, checked_path(argument)?)?,
        }
    }
    Ok(Command::Scan(ScanOptions {
        path: path.unwrap_or_else(|| PathBuf::from(".")),
        selection: selection.unwrap_or(Selection::Depth(1)),
        metric: metric.unwrap_or_default(),
        json,
        excludes,
    }))
}

fn set_path(current: &mut Option<PathBuf>, path: PathBuf) -> Result<(), String> {
    if current.is_some() {
        return Err("Provide at most one path.".into());
    }
    *current = Some(path);
    Ok(())
}

fn checked_path(path: OsString) -> Result<PathBuf, String> {
    if path.is_empty() {
        Err("Provide a nonempty path.".into())
    } else {
        Ok(path.into())
    }
}

fn is_option(value: &OsStr) -> bool {
    value.as_encoded_bytes().starts_with(b"-")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Result<Command, String> {
        parse(values.iter().map(OsString::from))
    }

    #[test]
    fn opens_the_current_directory_by_default() {
        let explore = |path: &str| {
            Ok(Command::Explore(ExploreOptions {
                path: path.into(),
                excludes: ExcludeOptions::default(),
                no_color: false,
            }))
        };
        assert_eq!(args(&[]), explore("."));
        assert_eq!(args(&["--", "-data"]), explore("-data"));
        assert_eq!(
            args(&["scan", "--json", "--top", "0"]),
            Ok(Command::Scan(ScanOptions {
                path: ".".into(),
                selection: Selection::Top(0),
                metric: Metric::Allocated,
                json: true,
                excludes: ExcludeOptions::default(),
            }))
        );
        assert_eq!(
            args(&["scan", "--apparent", "data"]),
            Ok(Command::Scan(ScanOptions {
                path: "data".into(),
                selection: Selection::Depth(1),
                metric: Metric::Apparent,
                json: false,
                excludes: ExcludeOptions::default(),
            }))
        );
    }

    #[test]
    fn collects_exclude_patterns_and_files() {
        let excludes = ExcludeOptions {
            patterns: vec!["node_modules".into(), "*.log".into()],
            files: vec!["ignore.txt".into()],
        };
        assert_eq!(
            args(&[
                "--exclude",
                "node_modules",
                "data",
                "--exclude-from",
                "ignore.txt",
                "--exclude",
                "*.log",
            ]),
            Ok(Command::Explore(ExploreOptions {
                path: "data".into(),
                excludes,
                no_color: false,
            }))
        );
        assert!(matches!(
            args(&["--no-color", "data"]),
            Ok(Command::Explore(ExploreOptions { no_color: true, .. }))
        ));
        let Ok(Command::Scan(scan)) = args(&["scan", "--exclude", "target", "--json"]) else {
            panic!("scan with excludes should parse");
        };
        assert_eq!(scan.excludes.patterns, ["target"]);
        for invalid in [
            &["--exclude"][..],
            &["scan", "--exclude", ""],
            &["scan", "--exclude-from"],
        ] {
            assert!(args(invalid).is_err(), "{invalid:?}");
        }
    }

    #[test]
    fn rejects_conflicting_sizes_or_invalid_paths() {
        for invalid in [
            &["scan", "--depth", "2", "--top", "3"][..],
            &["scan", "--metric", "logical"],
            &["scan", "--apparent", "--apparent"],
            &["scan", "--depth", "-1"],
            &["scan", "./one", "--", "-two"],
            &["scan", "--"],
            &[""],
        ] {
            assert!(args(invalid).is_err(), "{invalid:?}");
        }
    }
}
