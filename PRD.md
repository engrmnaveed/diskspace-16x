# Space 16X — Product requirements

**Owner** 16X Brains  
**Status** Updated for v0.2 · 19 November 2026\
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
| Automate | Provide `space16x scan [PATH] [--depth N | --top N] [--json]`. Use one level by default and emit plain text when the terminal explorer is invoked with redirected input or output. JSON schema 2 carries the scope, allocated bytes, the requested depth or top limit, and scan issues. |
| Scan correctly | Walk metadata only. Skip symbolic links and different-device mounts. Keep macOS cloud-only placeholders as references with zero counted bytes and leave their folder contents unexpanded. Count each hard-linked file's bytes once within the scan. Continue past unreadable descendants and fail clearly when the root itself is unreadable. |
| Explain disk usage | Directory totals sum counted regular-file bytes. Allocated bytes are reported by the filesystem. APFS clones may share blocks, snapshots may retain blocks, and scan totals can differ from volume usage or space recovered after deletion. |
| Preserve user control | The analyzer is read-only. The app and CLI keep scan results in memory, with JSON written by the CLI on request. File management happens in Finder. |

Defer automatic cleanup, duplicate hashing, background monitoring, network scans, and full-volume storage reconciliation. Validate behavior with actual cloud providers before promising offline directory enumeration.

## macOS premium hypothesis

Keep the ordinary scanning, browsing, and export workflow useful in the free edition. Test demand for a dedicated **Build Footprint** workflow in the Mac app.

1. Choose project and optional related output folders. Save a baseline and name the operation being measured, such as “first Xcode build” or “install JS dependencies.”
2. Run the operation independently, then capture a second scan. Show gained and removed bytes per root, rank changed paths, and flag scope or permission differences that make comparisons unreliable.
3. Save a local receipt that can be reopened or exported with relative paths and optional path redaction. A later experiment can add reusable size budgets for recurring builds.

The comparison measures change during the selected interval. Other processes can change the same folders during that interval, so a receipt must show the scan times and avoid asserting process ownership. Capture and review should work offline. Validate the idea with real builds and installs before implementing payments.

Radix already offers free scan comparisons, and StorageRadar documents before-and-after analysis. The proposed differentiation is a repeatable, operation-labelled workflow across project and output locations with a shareable receipt. Market uniqueness remains unverified. [Radix](https://tryradix.app/) · [StorageRadar](https://storageradar.app/blog/how-to-compare-disk-usage-over-time-on-mac/)

### Pricing hypothesis

Test **US$4.99 one-time** first. It is about half of DaisyDisk’s US$9.99 lifetime price. If ongoing paid work warrants a subscription, test up to **US$4.99 per year**, about half of Nektony’s listed US$9.95 annual price. Keep **US$9.99 lifetime** as an alternative if premium proves valuable enough to price alongside DaisyDisk. Launch with one pricing model after validation. The Linux CLI has no paid edition. [DaisyDisk pricing](https://daisydiskapp.com/?source=mas) · [Nektony pricing](https://nektony.com/disk-expert/buy)

## Architecture and delivery

Keep the SwiftPM scanner, SwiftUI, and AppKit in the native app. Build the portable CLI with Cargo and the pinned Rust toolchain. Apply the same allocated-size accounting rules in each scanner and compare stable fixtures across the products. Use macOS 14 as the deployment floor and validate supported versions before release. Prepare native CLI archives for macOS Apple silicon and Intel plus static musl archives for Linux x86-64 and ARM64. Provide a checksummed, per-user GitHub release installer and source instructions for other Linux architectures. Keep release packaging and any future premium entitlement code out of the scanners.

| Milestone | Done when |
| --- | --- |
| Local prototype | A fixture can be scanned from the CLI and explored in a native Mac window. Verify parity with known sizes, hard links, symlinks, sparse files, and partial results. |
| Community preview | Drag-and-drop, navigation, accessibility, and cancellation pass manual checks. CI runs the Rust CLI, TUI, and fixtures on both macOS and Linux architectures. Document scope and version support, record scan time and peak memory on a reproducible large tree, and publish source and build instructions after owner review. |
| Public release | Ship a signed, notarized macOS app and tested CLI artifacts for the release target matrix. Publish an installation guide, an example report, and contribution issues. |
| Premium discovery | Observe at least three developers comparing real build or install footprints. Verify that the receipt changes an actual decision before building licensing or billing. |

## v0.2 acceptance

- A user can locate a known large file inside a nested fixture and reveal it in Finder using keyboard or pointer.
- With identical scope, stable fixture paths produce matching allocated sizes in the desktop app and CLI and rank consistently, including ties.
- A partial scan reports skipped paths and clearly qualifies its totals. The CLI distinguishes complete scans, partial results, and fatal errors.
- The CLI can browse and filter a scanned directory in an interactive terminal. Plain text and JSON work in scripts. Verify release checksums and install without administrator privileges.
- Report scan speed and peak memory using the same fixture on named hardware before making performance claims. Cancellation keeps the Mac interface responsive.

## Decisions before a public release

Confirm the final name, runtime coverage for the candidate release matrix, release channels, and the date to add the project to the 16X Brains site. No pricing, payment, or distribution commitments are needed for the prototype.
