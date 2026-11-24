# DiskUse16x — Product requirements

**Owner** 16X Brains  
**Status** Updated for 1.0 · 24 November 2026\
**Local project** Disk Space Analyzer 16X  
**Product name** DiskUse16x, decided 24 November 2026 (formerly Space 16X) · CLI `diskuse16x`, short name `du16x`  
**Company** [16X Brains](https://16xbrains.com/)

## Product

DiskUse16x is a Mac-first disk space analyzer that helps someone find the file or folder responsible for a storage problem. The native macOS app is the primary product. The Rust CLI also targets Linux and provides a terminal explorer and scriptable reports. It was called Space 16X until the owner named it DiskUse16x before 1.0. It carries a simple 16X Brains credit.

## Users and their jobs

- **Mac users running low on storage** want to find large items in Downloads, their home folder, or an external drive and review them in Finder.
- **Developers and power users** want consistent results from the desktop and terminal, plus structured output for scripts.
- **Open-source contributors** need a small, reproducible project with fixtures for filesystem edge cases and a clear way to propose changes.

## Free edition

The Mac app, the CLI, and their scanners are free and open source. The CLI stays free on both macOS and Linux. The project uses the [MIT License](LICENSE). Both desktop and CLI work offline without an account.

| Area | Requirement |
| --- | --- |
| Start a scan | Choose a folder or mounted local drive using the Mac folder picker or Finder drag-and-drop. The CLI accepts a directory path and defaults to the current directory. Ordinary readable folders work with standard user permissions. |
| Browse | Rank the Mac app's list, treemap, donut, and largest-files view by allocated size. Keep drill-down and breadcrumb navigation accessible by keyboard. The terminal explorer opens in a TTY and offers folder navigation, search filtering, and largest files by allocated bytes. |
| Inspect | Reveal an item in Finder and copy its path. Show the first two levels while deeper scanning continues, with overall progress, folder status, cancellation, and error details. |
| Automate | Provide `diskuse16x scan [PATH] [--depth N | --top N] [--apparent] [--json] [--exclude PATTERN]`. Use one level by default and emit plain text when the terminal explorer is invoked with redirected input or output. JSON schema 3 carries the scope, allocated and apparent bytes, item counts, exclusions, the requested depth or top limit, and scan issues. |
| Focus (v0.3) | Skip files and folders by exclude pattern in both apps without reading them. Count the items inside each folder. Sort by size, item count, or name. Switch between allocated and apparent sizes. Honor `NO_COLOR` in the terminal explorer. |
| Scan correctly | Walk metadata only. Skip symbolic links and different-device mounts. Keep macOS cloud-only placeholders as references with zero counted bytes and leave their folder contents unexpanded. Count each hard-linked file's bytes once within the scan. Continue past unreadable descendants and fail clearly when the root itself is unreadable. |
| Explain disk usage | Directory totals sum counted regular-file bytes. Allocated bytes are reported by the filesystem. APFS clones may share blocks, snapshots may retain blocks, and scan totals can differ from volume usage or space recovered after deletion. |
| Act (v0.4) | Move selected items to the Trash after a confirmation that shows the item count and size, and update the results in place. Rescan one folder without rescanning everything. Preview files with Quick Look in the Mac app. |
| Preserve user control | The scanners are read-only and read metadata only. The app and CLI keep scan results in memory, with JSON reports, scan exports and Pro's saved history written on request. The only change to the user's files is Move to Trash, for items the user selects and confirms. It moves them to the Trash on their own disk and never deletes permanently. The CLI's `--read-only` option and `DISKUSE16X_READ_ONLY=1` turn it off. Other file management happens in Finder. |

Defer automatic cleanup, network scans, and full-volume storage reconciliation. Duplicate hashing and background monitoring, deferred until 0.8, now ship in Pro: `diskuse16x duplicates` and **Find Duplicates** read file contents only when the user starts them, and the menu-bar free-space monitor runs only while its setting is on. Validate behavior with actual cloud providers before promising offline directory enumeration.

## Editions

DiskUse16x ships two free, MIT-licensed editions from this repository. The [roadmap](docs/roadmap.md) records the competitor scan, the user requests behind each feature, and the milestones.

- **Basic** answers one question: what is using my space, and can I get rid of it? It stays light enough to run as a CLI on remote servers.
- **Pro** adds the features that cost something: extra per-item memory, a new dependency, state kept on disk, background work, reading file contents, or a significant amount of UI.

The CLI builds both editions from one crate, and both install the same `diskuse16x` and `du16x` commands. The Mac app is a single app with an off-by-default **Pro features** setting. Pro ships scan comparison and history, file types and ages, owners (CLI), cleanup suggestions, duplicates, exclusive sizes, disk explanation, the HTML report, the menu-bar monitor, and the **Build Footprint** receipt with size budgets. There is no paid tier, licensing, or payment code.

## Architecture and delivery

Keep the SwiftPM scanner, SwiftUI, and AppKit in the native app. Build the portable CLI with Cargo and the pinned Rust toolchain. Apply the same allocated-size accounting rules in each scanner and compare stable fixtures across the products. Use macOS 14 as the deployment floor and validate supported versions before release. Prepare native CLI archives for macOS Apple silicon and Intel plus static musl archives for Linux x86-64 and ARM64. Provide a checksummed, per-user GitHub release installer, Homebrew formulas and a cask in the project's tap, `.deb` and `.rpm` packages, the crate on crates.io, and source instructions for other Linux architectures. Every channel costs nothing. Keep release packaging and edition-specific code out of the scanners' core accounting.

| Milestone | Done when |
| --- | --- |
| Local prototype | A fixture can be scanned from the CLI and explored in a native Mac window. Verify parity with known sizes, hard links, symlinks, sparse files, and partial results. |
| Community preview | Drag-and-drop, navigation, accessibility, and cancellation pass manual checks. CI runs the Rust CLI, TUI, and fixtures on both macOS and Linux architectures. Document scope and version support, record scan time and peak memory on a reproducible large tree, and publish source and build instructions after owner review. |
| Public release (1.0) | Ship the ad hoc signed Mac app and tested CLI artifacts for the release target matrix, with SHA-256 files and build-provenance attestations, through the free channels above. Publish an installation guide with update steps, a user guide, an example report, a stable 1.x CLI contract, and contribution issues. |
| Pro edition | Build both CLI editions in CI, publish their archives from the same tag, and switch between them with the installer. Ship scan comparison as the first Pro feature, then the rest of the roadmap's Pro list. |

## v0.2 acceptance

The first preview's acceptance criteria, kept as the baseline every later release still meets.


- A user can locate a known large file inside a nested fixture and reveal it in Finder using keyboard or pointer.
- With identical scope, stable fixture paths produce matching allocated sizes in the desktop app and CLI and rank consistently, including ties.
- A partial scan reports skipped paths and clearly qualifies its totals. The CLI distinguishes complete scans, partial results, and fatal errors.
- The CLI can browse and filter a scanned directory in an interactive terminal. Plain text and JSON work in scripts. Verify release checksums and install without administrator privileges.
- Report scan speed and peak memory using the same fixture on named hardware before making performance claims. Cancellation keeps the Mac interface responsive.

## Decisions for 1.0

The owner decided these on 24 November 2026. The [roadmap](docs/roadmap.md) records each one under M8.

- **Name:** DiskUse16x, with the commands `diskuse16x` and `du16x` and the bundle ID `com.16xbrains.diskuse16x`. Space 16X users switch on upgrade and keep their settings and saved scans.
- **Notarization:** none for now. The Apple Developer Program costs about $99 a year, which waits until the tools have users. The app stays ad hoc signed, and the docs put the easiest first launch first.
- **Release channels:** only free ones: GitHub releases, the installer (which also installs the Mac app), the Homebrew tap, Linux packages and crates.io.
- **Basic's size budget:** 600 KB for the Basic archive, unchanged.

Still open for the owner: runtime coverage that needs hardware or a macOS 14 image (M8.2), the crates.io token, and the date to add the project to the 16X Brains site. No pricing or payment commitments are needed.
