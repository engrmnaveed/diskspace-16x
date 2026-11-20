# Changelog

## Unreleased

- Add apparent sizes and item counts to the CLI. `space16x scan --apparent` shows and ranks file lengths, as `ls -l` reports them, instead of allocated bytes, which helps with sparse and compressed files. JSON reports move to schema 3, which keeps every schema 2 field and adds `apparentBytes`, `itemCount` (files and folders below an entry, counting each hard-linked file once), `rankedBy` and `edition`. Exit codes are unchanged.
- Skip files and folders by pattern in the CLI explorer and reports with `--exclude PATTERN` and `--exclude-from FILE`. Patterns follow a small `.gitignore` subset: `node_modules` or `*.log` match a name anywhere, `/build` anchors to the scan root, `**` spans folders, and `cache/` matches folders only. Excluded entries aren't read, keep the scan complete, and appear as `excludedCount` and `excludes` in JSON.

## 0.2.4

- Treat folders the system won't let apps read as protected items rather than scan errors. A scan of `/` used to show an orange "Partial scan" with hundreds of skipped paths, though nearly all were expected macOS restrictions. Both apps now keep the orange warning for real errors and show a neutral notice for protected items. The Mac app groups them by folder in Scan Details and offers Full Disk Access when it would unlock some of them. The CLI summarizes them in one stderr line and marks them `[protected]` and Protected in the explorer. JSON output and exit codes are unchanged.
- Show CLI sizes in decimal units (KB, MB, GB), the way Finder, System Settings, and the Mac app do, instead of binary KiB, MiB, and GiB. The same folder now shows the same number everywhere, for example 245.33 GB rather than 228.5 GiB. The CLI explorer's file details and the Mac app's size tooltips also show the binary size next to the exact byte count. JSON reports are unchanged and still give exact bytes.

## 0.2.3

- Stop double-counting the macOS startup disk. A scan of `/` reached each APFS firmlinked folder, such as `/Users`, both at its visible path and again under `/System/Volumes/Data`, which roughly doubled the total. Both scanners now count those folders once at their visible paths.
- Spell out the first launch of the Mac app in the README, the installation guide, and the Homebrew caveats: click Done rather than Move to Trash, find Open Anyway under Security, and confirm with a password or Touch ID.

## 0.2.2

- Complete scans of very deep folder trees. Both scanners raise the open-file soft limit, which macOS sets to 256 for apps launched by launchd, so deep trees are no longer reported as partial with "Too many open files".
- Escape Unicode bidirectional control characters in terminal paths and messages, so a crafted filename can't display as different text.
- Restore the terminal before printing a panic message from the explorer.
- Publish `install.sh` with each CLI release and attest build provenance for the archives and installer. Pin the one-liners to a reviewed installer commit while the current release is available. Use the `release` environment to apply publication rules.
- Add a security policy, Dependabot updates, and a cargo-deny check for advisories, licenses, and dependency sources.
- Publish the universal Mac app with each release as an attested, ad hoc signed ZIP, along with a Homebrew cask. Install it with `brew install --cask engrmnaveed/tap/space16x`. The app isn't notarized, so the installation guide explains the one-time approval on first launch.
- Build the universal Mac app archive and check the rendered Homebrew cask with `brew style` on every pull request, so packaging problems surface before a release tag.

## 0.2.1

- Install or remove the CLI with a single curl or GNU Wget command, including setup and cleanup for PATH.
- Publish checksummed CLI binaries for macOS and Linux from version tags.

## 0.2.0 · Local release candidate

- Rust CLI with an interactive Ratatui explorer on macOS and Linux, including allocated-size browsing, filter, largest files, scan details, and cancellation.
- One-level terminal reports by default, with depth and largest-file options. JSON schema 2 reports allocated bytes and preserves undecodable path bytes where present.
- Candidate CLI archives for Apple silicon, Intel Mac, Linux x86-64, and Linux ARM64. The GitHub release installer verifies SHA-256 and supports per-user installation.

## 0.1.1 · Local update

- Avoid macOS cloud downloads during scanning. Show cloud-only items as references with zero counted bytes, and continue past stalled provider metadata with a partial-scan report.
- Show a two-level folder preview during larger scans. Update completed folders in the background while the results remain browsable.
- Use allocated size throughout the Mac app and add a donut chart for the selected folder's usage.

## 0.1.0 · Local release candidate

- Native macOS folder and local-drive scanning with Finder selection and drag-and-drop.
- Directory navigation, a folder treemap, allocated and logical size rankings, largest files, Finder reveal, and copy path.
- Scan progress, cancellation, and details for skipped paths.
- Shared Swift scanner with metadata-only traversal, hard-link accounting, and cross-device exclusions.
- macOS and Linux CLI with a versioned JSON format and distinct complete, partial, and fatal exit codes.
- Reproducible test fixtures and benchmark, plus macOS and Ubuntu release packaging.
