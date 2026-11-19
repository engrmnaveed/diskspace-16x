# Changelog

## Unreleased

- Complete scans of very deep folder trees. Both scanners raise the open-file soft limit, which macOS sets to 256 for apps launched by launchd, so deep trees are no longer reported as partial with "Too many open files".
- Escape Unicode bidirectional control characters in terminal paths and messages, so a crafted filename can't display as different text.
- Restore the terminal before printing a panic message from the explorer.
- Publish `install.sh` with each CLI release and attest build provenance for the archives and installer. Pin the one-liners to a reviewed installer commit while the current release is available. Use the `release` environment to apply publication rules.
- Add a security policy, Dependabot updates, and a cargo-deny check for advisories, licenses, and dependency sources.

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
