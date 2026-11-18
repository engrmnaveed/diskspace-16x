# Space 16X — Product requirements

**Owner** 16X Brains  
**Status** Initial draft · 18 November 2026  
**Local project** Disk Space Analyzer 16X  
**Working product name** Space 16X · CLI executable `space16x`  
**Alternate names** DiskScope 16X, Disk Atlas 16X  
**Company** [16X Brains](https://16xbrains.com/)

## Product

Space 16X is a Mac-first disk space analyzer that helps someone find the file or folder responsible for a storage problem. The native macOS app is the primary product. The shared scanner and CLI also target Linux. Use the working product name and a simple 16X Brains credit from day one. Decide on the final name, identity, and company-site placement before public launch.

## Users and their jobs

- **Mac users running low on storage** want to find large items in Downloads, their home folder, or an external drive and review them in Finder.
- **Developers and power users** want consistent results from the desktop and terminal, plus structured output for scripts.
- **Open-source contributors** need a small, reproducible project with fixtures for filesystem edge cases and a clear way to propose changes.

## Free edition

The core scanner, macOS Community app, and CLI are free and open source. The CLI stays free on both macOS and Linux. The proposed public source license is MIT, subject to owner confirmation before publication. Both desktop and CLI work offline without an account.

| Area | v0.1 requirement |
| --- | --- |
| Start a scan | Choose a folder or mounted local drive using the Mac folder picker or Finder drag-and-drop. The CLI takes a folder path. Ordinary readable folders work with standard user permissions. |
| Browse | Rank the Mac app's list, treemap, donut, and largest-files view by allocated size. Keep drill-down and breadcrumb navigation accessible by keyboard. |
| Inspect | Reveal an item in Finder and copy its path. Show the first two levels while deeper scanning continues, with overall progress, folder status, cancellation, and error details. |
| Automate | Provide `space16x scan PATH --top N` and `--json`. JSON includes the scan scope, byte counts, the top-file limit, and whether errors made the results partial. |
| Scan correctly | Walk metadata only. Skip symbolic links and different-device mounts. Keep macOS cloud-only placeholders as references with zero counted bytes and leave their folder contents unexpanded. Count each hard-linked file's bytes once within the scan. Continue past unreadable descendants and fail clearly when the root itself is unreadable. |
| Explain disk usage | Directory totals sum counted regular-file bytes. Allocated bytes are reported by the filesystem. APFS clones may share blocks, snapshots may retain blocks, and scan totals can differ from volume usage or space recovered after deletion. |
| Preserve user control | The analyzer is read-only. v0.1 keeps scan results in memory, with JSON written by the CLI on request. File management happens in Finder. |

Defer automatic cleanup, duplicate hashing, background monitoring, network scans, and full-volume storage reconciliation. Validate behavior with actual cloud providers before promising offline directory enumeration.

## macOS premium hypothesis

Keep the ordinary scanning, browsing, and export workflow useful in the free edition. Test demand for a dedicated **Build Footprint** workflow in the Mac app:

1. Choose project and optional related output folders. Save a baseline and name the operation being measured, such as “first Xcode build” or “install JS dependencies.”
2. Run the operation independently, then capture a second scan. Show gained and removed bytes per root, rank changed paths, and flag scope or permission differences that make comparisons unreliable.
3. Save a local receipt that can be reopened or exported with relative paths and optional path redaction. A later experiment can add reusable size budgets for recurring builds.

The comparison measures change during the selected interval. Other processes can change the same folders during that interval, so a receipt must show the scan times and avoid asserting process ownership. Capture and review should work offline. Validate the idea with real builds and installs before implementing payments.

Radix already offers free scan comparisons, and StorageRadar documents before-and-after analysis. The proposed differentiation is a repeatable, operation-labelled workflow across project and output locations with a shareable receipt. Market uniqueness remains unverified. [Radix](https://tryradix.app/) · [StorageRadar](https://storageradar.app/blog/how-to-compare-disk-usage-over-time-on-mac/)

### Pricing hypothesis

Test **US$4.99 one-time** first. It is about half of DaisyDisk’s US$9.99 lifetime price. If ongoing paid work warrants a subscription, test up to **US$4.99 per year**, about half of Nektony’s listed US$9.95 annual price. Keep **US$9.99 lifetime** as an alternative if premium proves valuable enough to price alongside DaisyDisk. Launch with one pricing model after validation. The Linux CLI has no paid edition. [DaisyDisk pricing](https://daisydiskapp.com/?source=mas) · [Nektony pricing](https://nektony.com/disk-expert/buy)

## Architecture and delivery

Use SwiftPM with a shared scanner and a portable CLI. Keep SwiftUI and AppKit in a macOS-only target. Start with macOS 14 as the deployment floor and validate supported macOS versions before release. Confirm the first supported Linux distribution and architecture through CI before advertising downloads. Keep release packaging and any future premium entitlement code out of the scanner.

| Milestone | Done when |
| --- | --- |
| Local prototype | A fixture can be scanned from the CLI and explored in a native Mac window. Both surfaces use the same scanner. Test known sizes, hard links, symlinks, sparse files, and partial results. |
| Community preview | Drag-and-drop, navigation, accessibility, and cancellation pass manual checks. Linux CI runs the CLI and fixtures. Document scope and version support, record scan time and peak memory on a reproducible large tree, and publish source and build instructions after owner review. |
| Public v0.1 | Ship a signed, notarized macOS build and a tested Linux CLI artifact. Publish an installation guide, an example report, and contribution issues. |
| Premium discovery | Observe at least three developers comparing real build or install footprints. Verify that the receipt changes an actual decision before building licensing or billing. |

## v0.1 acceptance

- A user can locate a known large file inside a nested fixture and reveal it in Finder using keyboard or pointer.
- With identical scope and byte metric, stable fixture paths produce matching desktop and CLI sizes and rank consistently, including ties.
- A partial scan reports skipped paths and clearly qualifies its totals. The CLI distinguishes complete scans, partial results, and fatal errors.
- Report scan speed and peak memory using the same fixture on named hardware before making performance claims. Cancellation keeps the Mac interface responsive.

## Decisions before a public release

Confirm the final name, open-source license, minimum macOS version and architectures, Linux support matrix, release channels, and the date to add the project to the 16X Brains site. No pricing, payment, or distribution commitments are needed for the prototype.
