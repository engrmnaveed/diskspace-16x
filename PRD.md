# Space 16X — Product requirements

**Owner** 16X Brains  
**Status** Updated for v0.3 · 21 November 2026\
**Local project** Disk Space Analyzer 16X  
**Working product name** Space 16X · CLI executable `space16x`  
**Alternate names** DiskScope 16X, Disk Atlas 16X  
**Company** [16X Brains](https://16xbrains.com/)

## Product

Space 16X is a Mac-first disk space analyzer that helps someone find the file or folder responsible for a storage problem. The native macOS app is the primary product. The Rust CLI also targets Linux and provides a terminal explorer and scriptable reports. Use the working product name and a simple 16X Brains credit from day one. Decide on the final name, identity, and company-site placement before public launch.

## Users and their jobs

- **Mac users running low on storage** want to find large items in Downloads, their home folder, or an external drive and review them in Finder.
- **Developers and power users** want consistent results from the desktop and terminal, plus structured output for scripts.
- **Open-source contributors** need a small, reproducible project with fixtures for filesystem edge cases and a clear way to propose changes.

## Free edition

The macOS Community app, CLI, and their scanners are free and open source. The CLI stays free on both macOS and Linux. The project uses the [MIT License](LICENSE). Both desktop and CLI work offline without an account.

| Area | v0.2 requirement |
| --- | --- |
| Start a scan | Choose a folder or mounted local drive using the Mac folder picker or Finder drag-and-drop. The CLI accepts a directory path and defaults to the current directory. Ordinary readable folders work with standard user permissions. |
| Browse | Rank the Mac app's list, treemap, donut, and largest-files view by allocated size. Keep drill-down and breadcrumb navigation accessible by keyboard. The terminal explorer opens in a TTY and offers folder navigation, search filtering, and largest files by allocated bytes. |
| Inspect | Reveal an item in Finder and copy its path. Show the first two levels while deeper scanning continues, with overall progress, folder status, cancellation, and error details. |
| Automate | Provide `space16x scan [PATH] [--depth N | --top N] [--apparent] [--json] [--exclude PATTERN]`. Use one level by default and emit plain text when the terminal explorer is invoked with redirected input or output. JSON schema 3 carries the scope, allocated and apparent bytes, item counts, exclusions, the requested depth or top limit, and scan issues. |
| Focus (v0.3) | Skip files and folders by exclude pattern in both apps without reading them. Count the items inside each folder. Sort by size, item count, or name. Switch between allocated and apparent sizes. Honor `NO_COLOR` in the terminal explorer. |
| Scan correctly | Walk metadata only. Skip symbolic links and different-device mounts. Keep macOS cloud-only placeholders as references with zero counted bytes and leave their folder contents unexpanded. Count each hard-linked file's bytes once within the scan. Continue past unreadable descendants and fail clearly when the root itself is unreadable. |
| Explain disk usage | Directory totals sum counted regular-file bytes. Allocated bytes are reported by the filesystem. APFS clones may share blocks, snapshots may retain blocks, and scan totals can differ from volume usage or space recovered after deletion. |
| Act (v0.4) | Move selected items to the Trash after a confirmation that shows the item count and size, and update the results in place. Rescan one folder without rescanning everything. Preview files with Quick Look in the Mac app. |
| Preserve user control | The scanners are read-only and read metadata only. The app and CLI keep scan results in memory, with JSON reports and scan exports written on request. The only change to the user's files is Move to Trash, for items the user selects and confirms. It moves them to the Trash on their own disk and never deletes permanently. The CLI's `--read-only` option and `SPACE16X_READ_ONLY=1` turn it off. Other file management happens in Finder. |

Defer automatic cleanup, duplicate hashing, background monitoring, network scans, and full-volume storage reconciliation. Validate behavior with actual cloud providers before promising offline directory enumeration.

## Editions

Space 16X ships two free, MIT-licensed editions from this repository. The [roadmap](docs/roadmap.md) records the competitor scan, the user requests behind each feature, and the milestones.

- **Basic** answers one question: what is using my space, and can I get rid of it? It stays light enough to run as a CLI on remote servers.
- **Pro** adds the features that cost something: extra per-item memory, a new dependency, state kept on disk, background work, reading file contents, or a significant amount of UI.

The CLI builds both editions from one crate, and both install the same `space16x` command. The Mac app is a single app with an off-by-default **Pro features** setting. Scan comparison, history, file types and ages, owners, cleanup suggestions, duplicates, and the **Build Footprint** receipt workflow are Pro candidates. There is no paid tier, licensing, or payment code.

## Architecture and delivery

Keep the SwiftPM scanner, SwiftUI, and AppKit in the native app. Build the portable CLI with Cargo and the pinned Rust toolchain. Apply the same allocated-size accounting rules in each scanner and compare stable fixtures across the products. Use macOS 14 as the deployment floor and validate supported versions before release. Prepare native CLI archives for macOS Apple silicon and Intel plus static musl archives for Linux x86-64 and ARM64. Provide a checksummed, per-user GitHub release installer and source instructions for other Linux architectures. Keep release packaging and edition-specific code out of the scanners' core accounting.

| Milestone | Done when |
| --- | --- |
| Local prototype | A fixture can be scanned from the CLI and explored in a native Mac window. Verify parity with known sizes, hard links, symlinks, sparse files, and partial results. |
| Community preview | Drag-and-drop, navigation, accessibility, and cancellation pass manual checks. CI runs the Rust CLI, TUI, and fixtures on both macOS and Linux architectures. Document scope and version support, record scan time and peak memory on a reproducible large tree, and publish source and build instructions after owner review. |
| Public release | Ship a signed, notarized macOS app and tested CLI artifacts for the release target matrix. Publish an installation guide, an example report, and contribution issues. |
| Pro edition | Build both CLI editions in CI, publish their archives from the same tag, and switch between them with the installer. Ship scan comparison as the first Pro feature, then the rest of the roadmap's Pro list. |

## v0.2 acceptance

- A user can locate a known large file inside a nested fixture and reveal it in Finder using keyboard or pointer.
- With identical scope, stable fixture paths produce matching allocated sizes in the desktop app and CLI and rank consistently, including ties.
- A partial scan reports skipped paths and clearly qualifies its totals. The CLI distinguishes complete scans, partial results, and fatal errors.
- The CLI can browse and filter a scanned directory in an interactive terminal. Plain text and JSON work in scripts. Verify release checksums and install without administrator privileges.
- Report scan speed and peak memory using the same fixture on named hardware before making performance claims. Cancellation keeps the Mac interface responsive.

## Decisions before a public release

Confirm the final name, runtime coverage for the candidate release matrix, release channels, and the date to add the project to the 16X Brains site. No pricing, payment, or distribution commitments are needed for the prototype.
