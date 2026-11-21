mod ncdu;
#[cfg(feature = "pro")]
mod pro;
mod report;
mod scanner;
mod trash;
mod tui;
mod volumes;

use std::env;
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::{self, IsTerminal, Read, Write};
use std::os::fd::{AsFd, AsRawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;

use report::{Metric, Selection};

const HELP: &str = "\
Usage: space16x [--no-color] [--read-only] [--threads N] [EXCLUDES] [PATH]
       space16x --import FILE|- [--no-color]
       space16x scan [PATH] [--depth N | --top N] [--apparent] [--json] [--export FILE|-]
                     [--threads N] [EXCLUDES]
       space16x volumes [--all] [--json]

Open the disk explorer in an interactive terminal. PATH defaults to the current directory.
In the explorer, Space marks items and d moves them to the Trash after you confirm.
The scan command prints a plain directory summary and works in scripts. The volumes
command shows the size, used and free space, and inode use of each mounted volume, and
--all adds system, memory and duplicate mounts.

  --depth N  List entries through N levels (default: 1, 0 for the root total)
  --top N    List the N largest files in the scan instead of directory entries
  --apparent Show and rank apparent sizes (file lengths) instead of disk usage
  --json     Print a report in JSON schema 3
  --export FILE Also save the whole scan to FILE in ncdu's JSON format. With -, write
             only the export to standard output.
  --import FILE Browse a scan saved by --export, ncdu -o or gdu -o, read-only. With -,
             read standard input: ssh HOST space16x scan / --export - | space16x --import -
  --no-color Show the explorer without colors, as does setting NO_COLOR
  --read-only Turn off Move to Trash in the explorer, as does SPACE16X_READ_ONLY=1
  --threads N Read metadata with N threads, from 1 to 64 (default: one per core, up
             to 8). Use 1 for hard disks and network filesystems.
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

#[cfg(not(feature = "pro"))]
const EDITION_HELP: &str = "
This is Space 16X Basic. Space 16X Pro, also free, adds space16x diff to compare scans,
modification dates, scan --older-than and scan --types. Install it with install.sh --pro.
See docs/install.md.
";

#[cfg(feature = "pro")]
const EDITION_HELP: &str = "
This is Space 16X Pro. It adds:
  space16x diff OLD NEW|PATH [--depth N | --top N] [--apparent] [--json] [--threads N]
             Compare a saved scan with another one, or with PATH scanned now using the
             saved scan's excludes. Lists what changed, or the N files that changed most.
  scan --older-than DAYS
             Count only files last modified more than DAYS days ago, and the folders
             that hold them. JSON reports add newestModified, as Unix seconds, to each
             entry: the file's time, or the newest time of any file in a folder.
  scan --types N
             List the N file types (extensions) that use the most space instead of
             entries, with their file counts.
The explorer adds a Modified column with each item's newest modification date, and t
lists the file types in the open folder.
";

/// Subcommands only Space 16X Pro has. Basic builds answer them with an install hint.
const PRO_COMMANDS: &[&str] = &["diff"];

/// Options only Space 16X Pro's scan takes. Basic builds answer them with an install hint.
const PRO_SCAN_OPTIONS: &[&str] = &["--older-than", "--types"];

#[cfg(feature = "pro")]
use pro::ScanOptions as ProScanOptions;
/// Basic has no Pro scan options.
#[cfg(not(feature = "pro"))]
#[derive(Debug, Default, PartialEq, Eq)]
struct ProScanOptions {}

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Help,
    Version,
    Explore(ExploreOptions),
    Scan(ScanOptions),
    Volumes {
        json: bool,
        all: bool,
    },
    #[cfg(feature = "pro")]
    Pro(pro::Command),
    /// A Pro subcommand run in a Basic build.
    #[cfg(not(feature = "pro"))]
    ProOnly(&'static str),
}

#[derive(Debug, PartialEq, Eq)]
struct ExploreOptions {
    path: PathBuf,
    import: Option<PathBuf>,
    excludes: ExcludeOptions,
    threads: Option<usize>,
    no_color: bool,
    read_only: bool,
}

#[derive(Debug, PartialEq, Eq)]
struct ScanOptions {
    path: PathBuf,
    selection: Selection,
    metric: Metric,
    json: bool,
    export: Option<PathBuf>,
    excludes: ExcludeOptions,
    threads: Option<usize>,
    #[cfg_attr(not(feature = "pro"), allow(dead_code))]
    pro: ProScanOptions,
}

/// The most threads `--threads` accepts.
const MAX_THREADS: usize = 64;

/// Takes the value after `--threads`.
fn take_threads(
    current: &mut Option<usize>,
    arguments: &mut impl Iterator<Item = OsString>,
) -> Result<(), String> {
    if current.is_some() {
        return Err("Use --threads once.".into());
    }
    let threads = arguments
        .next()
        .and_then(|value| value.to_str()?.parse::<usize>().ok())
        .filter(|threads| (1..=MAX_THREADS).contains(threads))
        .ok_or_else(|| format!("--threads requires a number from 1 to {MAX_THREADS}."))?;
    *current = Some(threads);
    Ok(())
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
        Command::Help => io::stdout().write_all([HELP, EDITION_HELP].concat().as_bytes())?,
        Command::Version => writeln!(
            io::stdout(),
            "space16x {}{}",
            env!("CARGO_PKG_VERSION"),
            if cfg!(feature = "pro") { " (pro)" } else { "" }
        )?,
        #[cfg(feature = "pro")]
        Command::Pro(command) => return pro::run(command),
        #[cfg(not(feature = "pro"))]
        Command::ProOnly(name) => {
            eprintln!(
                "space16x: {name} is in Space 16X Pro, a free edition. Install it with install.sh --pro: https://github.com/engrmnaveed/diskspace-16x/blob/main/docs/install.md#install-space-16x-pro"
            );
            return Ok(1);
        }
        Command::Explore(options) => return explore(options),
        Command::Scan(options) => return run_scan(options),
        Command::Volumes { json, all } => {
            let (list, issues) = volumes::list(all)?;
            if json {
                volumes::write_json(io::stdout().lock(), &list, &issues)?;
            } else {
                volumes::write_plain(io::stdout().lock(), &list)?;
                for (path, message) in &issues {
                    eprintln!(
                        "space16x: {}: {}",
                        report::terminal_path(path),
                        report::terminal_text(message)
                    );
                }
            }
            return Ok(if issues.is_empty() { 0 } else { 2 });
        }
    }
    Ok(0)
}

fn explore(options: ExploreOptions) -> io::Result<u8> {
    let imported = match &options.import {
        Some(file) => Some(import(file)?),
        None => None,
    };
    // Keys come from the terminal even when an import arrives on standard input.
    let from_stdin = options.import.as_deref() == Some(Path::new("-"));
    if !((from_stdin || io::stdin().is_terminal())
        && io::stdout().is_terminal()
        && env::var_os("TERM").is_none_or(|term| term != "dumb"))
    {
        let Some(imported) = imported else {
            return run_scan(ScanOptions {
                path: options.path,
                selection: Selection::Depth(1),
                metric: Metric::Allocated,
                json: false,
                export: None,
                excludes: options.excludes,
                threads: options.threads,
                pro: ProScanOptions::default(),
            });
        };
        let report = &imported.report;
        report::write_plain(
            io::stdout().lock(),
            report,
            Selection::Depth(1),
            Metric::Allocated,
        )?;
        report_issues(&report.issues, false);
        report_excluded(report.excluded_count, imported.excludes.len());
        return Ok(status(report));
    }
    if from_stdin && !io::stdin().is_terminal() {
        read_keys_from_terminal()?;
    }
    // https://no-color.org: any nonempty NO_COLOR value turns colors off.
    let plain = options.no_color || env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
    let read_only = options.read_only || trash::read_only_requested();
    let source = match imported {
        Some(imported) => tui::Source::Imported(imported.report),
        None => tui::Source::Scan {
            path: options.path,
            excludes: options.excludes.load()?,
            threads: options.threads.unwrap_or_else(scanner::default_threads),
        },
    };
    tui::run(source, plain, read_only)?;
    Ok(0)
}

/// Points standard input at the terminal on standard output, once an import has been
/// read from a pipe, so the explorer can read keys. crossterm could open /dev/tty
/// itself, but macOS can't watch that device for input.
fn read_keys_from_terminal() -> io::Result<()> {
    // SAFETY: ttyname returns null or a C string that stays valid until the next call,
    // and nothing else calls it.
    let name = unsafe { libc::ttyname(libc::STDOUT_FILENO) };
    if name.is_null() {
        return Err(io::Error::last_os_error());
    }
    let name = OsStr::from_bytes(unsafe { std::ffi::CStr::from_ptr(name) }.to_bytes());
    let terminal = File::options().read(true).write(true).open(name)?;
    // SAFETY: both descriptors are open, and dup2 replaces standard input atomically.
    if unsafe { libc::dup2(terminal.as_raw_fd(), libc::STDIN_FILENO) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Reads a saved scan from `file`, or from standard input for `-`.
fn import(file: &Path) -> io::Result<ncdu::Imported> {
    let (mut input, label) = if file == Path::new("-") {
        let stdin = io::stdin().as_fd().try_clone_to_owned()?;
        (File::from(stdin), "standard input".into())
    } else {
        let input = File::open(file).map_err(|error| scan_error(file, error))?;
        (input, report::terminal_path(file))
    };
    ncdu::read(&mut |buffer| input.read(buffer))
        .map_err(|error| io::Error::new(error.kind(), format!("{label}: {error}")))
}

fn run_scan(options: ScanOptions) -> io::Result<u8> {
    let excludes = options.excludes.load()?;
    let threads = options.threads.unwrap_or_else(scanner::default_threads);
    let scan = scanner::scan(
        &options.path,
        scanner::Options {
            excludes: &excludes,
            threads,
        },
        false,
        &AtomicBool::new(false),
        |_| {},
    )
    .map_err(|error| scan_error(&options.path, error))?;
    let sources = excludes.sources();
    match options.export.as_deref() {
        Some(file) if file == Path::new("-") => {
            let mut stdout = io::stdout().lock();
            ncdu::write(&mut |bytes| stdout.write_all(bytes), &scan, sources)?;
            stdout.flush()?;
            report_issues(&scan.issues, true);
            report_excluded(scan.excluded_count, sources.len());
            return Ok(status(&scan));
        }
        Some(file) => File::create(file)
            .and_then(|mut output| {
                ncdu::write(&mut |bytes| output.write_all(bytes), &scan, sources)
            })
            .map_err(|error| scan_error(file, error))?,
        None => {}
    }
    #[cfg(feature = "pro")]
    let (scan, written) = {
        let scan = options.pro.narrow(scan);
        let written = options
            .pro
            .write_report(&scan, options.metric, sources, options.json)?;
        (scan, written)
    };
    #[cfg(not(feature = "pro"))]
    let written = false;
    if !written && options.json {
        #[cfg(feature = "pro")]
        let older_than = options.pro.older_than_days();
        #[cfg(not(feature = "pro"))]
        let older_than = None;
        report::write_json(
            io::stdout().lock(),
            &scan,
            options.selection,
            options.metric,
            sources,
            older_than,
        )?;
    } else if !written {
        report::write_plain(
            io::stdout().lock(),
            &scan,
            options.selection,
            options.metric,
        )?;
    }
    if !options.json {
        report_issues(&scan.issues, true);
        report_excluded(scan.excluded_count, sources.len());
        #[cfg(feature = "pro")]
        options.pro.report_narrowed();
    }
    Ok(status(&scan))
}

/// Exit status 2 marks a partial scan.
fn status(report: &scanner::Report) -> u8 {
    if report.issues.is_empty() { 0 } else { 2 }
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
/// `local` scans can suggest Full Disk Access, which can't help an imported scan.
fn report_issues(issues: &[scanner::Issue], local: bool) {
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
    if local && privacy_refusals && !has_full_disk_access() {
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
        Some("volumes") => {
            arguments.next();
            return parse_volumes(arguments);
        }
        Some(name) if PRO_COMMANDS.contains(&name) => {
            let name = pro_name(PRO_COMMANDS, name);
            arguments.next();
            #[cfg(feature = "pro")]
            return pro::parse(name, arguments);
            #[cfg(not(feature = "pro"))]
            return Ok(Command::ProOnly(name));
        }
        _ => {}
    }

    parse_explore(arguments)
}

fn parse_explore(mut arguments: impl Iterator<Item = OsString>) -> Result<Command, String> {
    let mut path = None;
    let mut import = None;
    let mut excludes = ExcludeOptions::default();
    let mut no_color = false;
    let mut read_only = false;
    let mut threads = None;
    while let Some(argument) = arguments.next() {
        match argument.to_str() {
            Some("--") => {
                set_path(&mut path, single_path(arguments, true)?)?;
                break;
            }
            Some(flag @ ("--exclude" | "--exclude-from")) => excludes.take(flag, &mut arguments)?,
            Some("--no-color") if !no_color => no_color = true,
            Some("--read-only") if !read_only => read_only = true,
            Some("--threads") => take_threads(&mut threads, &mut arguments)?,
            Some("--import") if import.is_none() => import = Some(take_file(&mut arguments)?),
            _ if is_option(&argument) => return Err(format!("Unknown option {argument:?}.")),
            _ => set_path(&mut path, checked_path(argument)?)?,
        }
    }
    if import.is_some() && (path.is_some() || !excludes.is_empty() || threads.is_some()) {
        return Err(
            "--import opens a saved scan, so it takes no PATH, excludes or --threads.".into(),
        );
    }
    Ok(Command::Explore(ExploreOptions {
        path: path.unwrap_or_else(|| PathBuf::from(".")),
        import,
        excludes,
        threads,
        no_color,
        read_only,
    }))
}

/// Takes the file after `--import` or `--export`, where `-` is a standard stream.
fn take_file(arguments: &mut impl Iterator<Item = OsString>) -> Result<PathBuf, String> {
    arguments
        .next()
        .filter(|file| !file.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| "--import and --export require a file path or -.".into())
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
    let mut export = None;
    let mut excludes = ExcludeOptions::default();
    let mut threads = None;
    #[cfg_attr(not(feature = "pro"), allow(unused_mut))]
    let mut pro = ProScanOptions::default();
    while let Some(argument) = arguments.next() {
        match argument.to_str() {
            Some("-h" | "--help")
                if path.is_none()
                    && selection.is_none()
                    && metric.is_none()
                    && !json
                    && export.is_none()
                    && excludes.is_empty()
                    && threads.is_none()
                    && pro == ProScanOptions::default() =>
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
            Some("--export") if export.is_none() => export = Some(take_file(&mut arguments)?),
            Some("--apparent") if metric.is_none() => metric = Some(Metric::Apparent),
            Some(flag @ ("--exclude" | "--exclude-from")) => excludes.take(flag, &mut arguments)?,
            Some("--threads") => take_threads(&mut threads, &mut arguments)?,
            Some(flag) if PRO_SCAN_OPTIONS.contains(&flag) => {
                #[cfg(feature = "pro")]
                pro.take(flag, &mut arguments)?;
                #[cfg(not(feature = "pro"))]
                return Ok(Command::ProOnly(pro_name(PRO_SCAN_OPTIONS, flag)));
            }
            _ if is_option(&argument) => {
                return Err(format!("Unknown or repeated option {argument:?}."));
            }
            _ => set_path(&mut path, checked_path(argument)?)?,
        }
    }
    if export.as_deref() == Some(Path::new("-"))
        && (json || selection.is_some() || metric.is_some() || pro != ProScanOptions::default())
    {
        return Err(
            "--export - writes only the export, so it takes no report options such as --json or --depth."
                .into(),
        );
    }
    #[cfg(feature = "pro")]
    pro.check(selection)?;
    Ok(Command::Scan(ScanOptions {
        path: path.unwrap_or_else(|| PathBuf::from(".")),
        selection: selection.unwrap_or(Selection::Depth(1)),
        metric: metric.unwrap_or_default(),
        json,
        export,
        excludes,
        threads,
        pro,
    }))
}

/// The listed name of a Pro command or option, which the install hint repeats.
fn pro_name(names: &'static [&'static str], name: &str) -> &'static str {
    names
        .iter()
        .find(|listed| **listed == name)
        .expect("the name is listed")
}

fn parse_volumes(arguments: impl Iterator<Item = OsString>) -> Result<Command, String> {
    let (mut json, mut all) = (false, false);
    for argument in arguments {
        match argument.to_str() {
            Some("-h" | "--help") if !json && !all => return Ok(Command::Help),
            Some("--json") if !json => json = true,
            Some("--all") if !all => all = true,
            _ => return Err(format!("Unknown or repeated option {argument:?}.")),
        }
    }
    Ok(Command::Volumes { json, all })
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
                import: None,
                excludes: ExcludeOptions::default(),
                threads: None,
                no_color: false,
                read_only: false,
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
                export: None,
                excludes: ExcludeOptions::default(),
                threads: None,
                pro: ProScanOptions::default(),
            }))
        );
        assert_eq!(
            args(&["scan", "--apparent", "data"]),
            Ok(Command::Scan(ScanOptions {
                path: "data".into(),
                selection: Selection::Depth(1),
                metric: Metric::Apparent,
                json: false,
                export: None,
                excludes: ExcludeOptions::default(),
                threads: None,
                pro: ProScanOptions::default(),
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
                import: None,
                excludes,
                threads: None,
                no_color: false,
                read_only: false,
            }))
        );
        assert!(matches!(
            args(&["--no-color", "data"]),
            Ok(Command::Explore(ExploreOptions { no_color: true, .. }))
        ));
        assert!(matches!(
            args(&["--read-only", "data"]),
            Ok(Command::Explore(ExploreOptions {
                read_only: true,
                ..
            }))
        ));
        assert!(args(&["--read-only", "--read-only"]).is_err());
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
    fn takes_a_thread_count_from_1_to_64() {
        assert!(matches!(
            args(&["--threads", "1", "data"]),
            Ok(Command::Explore(ExploreOptions {
                threads: Some(1),
                ..
            }))
        ));
        let Ok(Command::Scan(scan)) = args(&["scan", "--threads", "64", "--json"]) else {
            panic!("scan with threads should parse");
        };
        assert_eq!(scan.threads, Some(64));
        for invalid in [
            &["--threads"][..],
            &["--threads", "0"],
            &["scan", "--threads", "65"],
            &["scan", "--threads", "two"],
            &["scan", "--threads", "2", "--threads", "2"],
        ] {
            assert!(args(invalid).is_err(), "{invalid:?}");
        }
    }

    #[test]
    fn takes_export_and_import_files() {
        let Ok(Command::Scan(scan)) = args(&["scan", "/", "--export", "-"]) else {
            panic!("scan with an export should parse");
        };
        assert_eq!(scan.export, Some("-".into()));
        let Ok(Command::Scan(scan)) =
            args(&["scan", "--export", "out.json", "--json", "--top", "3"])
        else {
            panic!("an export file allows any report");
        };
        assert_eq!(scan.export, Some("out.json".into()));
        assert!(matches!(
            args(&["--no-color", "--import", "-"]),
            Ok(Command::Explore(ExploreOptions { import: Some(file), no_color: true, .. }))
                if file == Path::new("-")
        ));
        for invalid in [
            &["scan", "--export"][..],
            &["scan", "--export", ""],
            &["scan", "--export", "a", "--export", "b"],
            &["scan", "--export", "-", "--json"],
            &["scan", "--apparent", "--export", "-"],
            &["scan", "--export", "-", "--depth", "2"],
            &["--import"],
            &["--import", "a", "--import", "b"],
            &["--import", "a", "data"],
            &["--import", "a", "--threads", "2"],
            &["--exclude", "x", "--import", "a"],
        ] {
            assert!(args(invalid).is_err(), "{invalid:?}");
        }
    }

    #[test]
    fn routes_pro_commands_by_edition() {
        #[cfg(not(feature = "pro"))]
        {
            assert_eq!(
                args(&["diff", "old.json", "new.json"]),
                Ok(Command::ProOnly("diff"))
            );
            assert_eq!(
                args(&["scan", "--older-than", "30"]),
                Ok(Command::ProOnly("--older-than"))
            );
            assert_eq!(
                args(&["scan", "--types", "10"]),
                Ok(Command::ProOnly("--types"))
            );
        }
        #[cfg(feature = "pro")]
        {
            assert!(matches!(
                args(&["diff", "old.json", "new.json"]),
                Ok(Command::Pro(_))
            ));
            let Ok(Command::Scan(scan)) = args(&["scan", "--older-than", "30", "--top", "5"])
            else {
                panic!("scan --older-than should parse in Pro");
            };
            assert_eq!(scan.pro.older_than_days(), Some(30));
            assert!(args(&["scan", "--types", "10", "--older-than", "9", "--json"]).is_ok());
            for invalid in [
                &["scan", "--types"][..],
                &["scan", "--types", "3", "--depth", "2"],
                &["scan", "--top", "3", "--types", "3"],
                &["scan", "--types", "1", "--types", "2"],
                &["scan", "--older-than"],
                &["scan", "--older-than", "-1"],
                &["scan", "--older-than", "1", "--older-than", "2"],
                &["scan", "--older-than", "1", "--export", "-"],
            ] {
                assert!(args(invalid).is_err(), "{invalid:?}");
            }
        }
        // A folder with a Pro command's name can still be explored.
        assert!(matches!(
            args(&["./diff"]),
            Ok(Command::Explore(ExploreOptions { path, .. })) if path == Path::new("./diff")
        ));
    }

    #[test]
    fn takes_volume_options() {
        assert_eq!(
            args(&["volumes"]),
            Ok(Command::Volumes {
                json: false,
                all: false
            })
        );
        assert_eq!(
            args(&["volumes", "--all", "--json"]),
            Ok(Command::Volumes {
                json: true,
                all: true
            })
        );
        for invalid in [&["volumes", "/"][..], &["volumes", "--json", "--json"]] {
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
