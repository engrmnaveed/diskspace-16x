mod report;
mod scanner;
mod tui;

use std::env;
use std::ffi::{OsStr, OsString};
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;

use report::Selection;

const HELP: &str = "\
Usage: space16x [PATH]
       space16x scan [PATH] [--depth N | --top N] [--json]

Open the disk explorer in an interactive terminal. PATH defaults to the current directory.
The scan command prints a plain directory summary and works in scripts.

  --depth N  List entries through N levels (default: 1, 0 for the root total)
  --top N    List the N largest files in the scan instead of directory entries
  --json     Print an allocated-size report in JSON schema 2
  --         Treat the following argument as a path, even if it starts with a dash
  --version  Show the installed version
  -h, --help Show this help

Sizes are allocated bytes in decimal units (1 KB = 1,000 bytes), as in Finder.
Scan exit codes: 0 complete, 1 error, 2 partial.
";

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Help,
    Version,
    Explore(PathBuf),
    Scan(ScanOptions),
}

#[derive(Debug, PartialEq, Eq)]
struct ScanOptions {
    path: PathBuf,
    selection: Selection,
    json: bool,
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
        Command::Explore(path)
            if io::stdin().is_terminal()
                && io::stdout().is_terminal()
                && env::var_os("TERM").is_none_or(|term| term != "dumb") =>
        {
            tui::run(&path)?;
        }
        Command::Explore(path) => {
            return run_scan(ScanOptions {
                path,
                selection: Selection::Depth(1),
                json: false,
            });
        }
        Command::Scan(options) => return run_scan(options),
    }
    Ok(0)
}

fn run_scan(options: ScanOptions) -> io::Result<u8> {
    let scan =
        scanner::scan(&options.path, false, &AtomicBool::new(false), |_| {}).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("{}: {error}", report::terminal_path(&options.path)),
            )
        })?;
    let stdout = io::stdout().lock();
    if options.json {
        report::write_json(stdout, &scan, options.selection)?;
    } else {
        report::write_plain(stdout, &scan, options.selection)?;
        report_issues(&scan.issues);
    }
    Ok(if scan.issues.is_empty() { 0 } else { 2 })
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

    if arguments.peek().is_some_and(|argument| argument == "--") {
        arguments.next();
        return single_path(arguments, true).map(Command::Explore);
    }
    single_path(arguments, false).map(Command::Explore)
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
    let mut json = false;
    while let Some(argument) = arguments.next() {
        match argument.to_str() {
            Some("-h" | "--help") if path.is_none() && selection.is_none() && !json => {
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
            _ if is_option(&argument) => {
                return Err(format!("Unknown or repeated option {argument:?}."));
            }
            _ => set_path(&mut path, checked_path(argument)?)?,
        }
    }
    Ok(Command::Scan(ScanOptions {
        path: path.unwrap_or_else(|| PathBuf::from(".")),
        selection: selection.unwrap_or(Selection::Depth(1)),
        json,
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
        assert_eq!(args(&[]), Ok(Command::Explore(PathBuf::from("."))));
        assert_eq!(
            args(&["--", "-data"]),
            Ok(Command::Explore(PathBuf::from("-data")))
        );
        assert_eq!(
            args(&["scan", "--json", "--top", "0"]),
            Ok(Command::Scan(ScanOptions {
                path: ".".into(),
                selection: Selection::Top(0),
                json: true,
            }))
        );
    }

    #[test]
    fn rejects_conflicting_sizes_or_invalid_paths() {
        for invalid in [
            &["scan", "--depth", "2", "--top", "3"][..],
            &["scan", "--metric", "logical"],
            &["scan", "--depth", "-1"],
            &["scan", "./one", "--", "-two"],
            &["scan", "--"],
            &[""],
        ] {
            assert!(args(invalid).is_err(), "{invalid:?}");
        }
    }
}
