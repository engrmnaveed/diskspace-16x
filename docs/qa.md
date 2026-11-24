# DiskUse16x verification

Releases before 1.0 were named Space 16X.

## 1.0.0 (in progress)

- **First launch through `install.sh --app`** (macOS 27.2.1, Apple silicon): a freshly built universal app, never launched before, was packaged by `scripts/archive-macos.sh` and fetched with curl. It passed its SHA-256 check, expanded with `ditto` with no `com.apple.quarantine` attribute anywhere in the bundle, and passed `codesign --verify --strict` (ad hoc). `spctl --assess --type execute` rejects it, which is the verdict a browser download gets, so that copy would need **Open Anyway**. Opened with `open`, the curl copy launched, created its window and kept running for nearly three minutes, and the system log shows no Gatekeeper decision for it. To repeat with the published v1.0.0 ZIP through the real installer.
- **Installer:** `scripts/check-install.py` covers `--app` with curl and wget: the latest version from GitHub's redirect, `--version`, replacing an earlier copy, refusing while the app runs, a checksum mismatch, a failed signature, a ZIP with another bundle ID, something else at `~/Applications/DiskUse16x.app`, `--uninstall --app`, and Linux. The rename's migration from a real pre-1.0 install was checked for bash, zsh and fish.
- **Accessibility (M8.2):** with the owner's screen access, the accessibility tree that VoiceOver reads was checked view by view on macOS 27.2.1, with a fixture scan. The start screen, the folder list, treemap, donut and Types, Largest Files, Cleanup, Duplicates (after **Find Duplicates**), Compare (with its icon-only **Dismiss message**, **Save Footprint Receipt** and **Stop comparing** buttons), and the Scan Details and Explain Disk Space sheets name every control. Charts that repeat what the list says, such as the donut ring and the share bars, are hidden or folded into a labelled row, as intended. **Fixed:** five controls had no name. These were the two switches and the threshold menu in Settings, which a grouped form draws apart from their labels, and the receipt sheet's two fields, whose prompts hide their titles. Each now has an explicit label, and a rebuilt app showed all five named.
- **Sizes:** the rename grew Basic's Linux x86-64 archive by 980 bytes, from 590,257 to 591,237, still 8,763 bytes under its 600 KB budget. Most of it is `cli.md`'s 1.x promise and rename notes, plus the `du16x` link entry. The macOS Basic binary is the same size.

### Owner checks (M8.1)

Use the 1.0 release build of DiskUse16x. Tick each line, or write what went wrong next to it.

1. [ ] **Start screen hint:** in **Settings**, turn **Pro features** off. On the start screen, click **Turn On in Settings…**: Settings opens at Pro features. Turn it off again, click the hint's **×**, quit and reopen: the hint stays hidden.
2. [ ] **Receipt sheet:** with Pro on, scan a folder, **Compare…** with a saved scan, then **Save Footprint Receipt…**. Type an operation name you used before: its budget fills in. Type `abc` as the budget: **Save…** is disabled.
3. [ ] **Save panels:** **File → Export HTML Report…** and the receipt's **Save…**. Click **Save** in each panel, then open both files.
4. [ ] **Time Machine snapshot:** run `tmutil localsnapshot` in Terminal, then **Scan → Explain Disk Space**: the snapshot is listed with today's date.
5. [ ] **Older-than menu:** **View → Show Files Older Than** lists Any Age, 30 and 90 days, 6 months, 1 and 2 years, and each one filters the list.
6. [ ] **Move to Trash alert:** select two items and press ⌘⌫. The alert names the count and the size and looks right in light and dark mode. Click **Cancel**.
7. [ ] **New in 1.0:** **Help → Check for Updates…** opens the newest release in your browser, and **Help → DiskUse16x Help** opens the user guide. Your Space 16X settings (Pro features, the menu-bar monitor) came across on the first launch. Turn on **DiskUse16x** in Full Disk Access.
8. [ ] **VoiceOver:** turn on VoiceOver (⌘F5), open **Settings** and press Tab through it: each switch and the threshold menu is announced with its label, such as "Turn on Pro features, switch". In the receipt sheet, the fields are announced as "Operation name" and "Budget, optional". Turn VoiceOver off with ⌘F5.

## 0.9.1

The Mac app's folder-list fix, on the same Mac as 0.9.0.

- All 63 Swift tests passed (42 in SpaceCore and 21 in SpacePro), and `scripts/package-macos.sh` built the app. The Rust CLI changed only its version.
- **Mac runtime QA.** The same temporary harness reproduced both "reentrant operation in its NSTableView delegate" warnings from 0.9.0, using a real APFS clone made with `cp -c`. With the fix, three runs logged no warning while setting and clearing the age filter, clicking the Exclusive and Items headers, and rescanning after files were added and removed. Each rebuilt list kept the selected row and the table's keyboard focus.
- Published release archives, Basic and Pro: `aarch64-apple-darwin` 511,486 and 725,366 bytes, `x86_64-apple-darwin` 548,991 and 796,103, `aarch64-unknown-linux-musl` 557,172 and 774,113, and `x86_64-unknown-linux-musl` 590,256 and 835,173. Basic's Linux x86-64 archive has about 9.7 KB left under the 600 KB budget.
- Publish release run 45025779369 attached all 21 assets on its second attempt. The first attempt stopped at build provenance, before anything was published, when Sigstore's transparency log returned 404 Not Found. The downloaded Apple silicon archives report `space16x 0.9.1` and `space16x 0.9.1 (pro)`, and the app reports 0.9.1. Their checksums and `gh attestation verify` pass, and the Homebrew tap's cask points at the 0.9.1 ZIP with its checksum.

## 0.9.0

Milestone 7 (Complete and stable) on the Apple M5 Pro MacBook Pro with macOS 27.2.1, APFS and Rust 1.100.0.

- The Rust CLI passed 73 unit tests in the Basic edition and 114 with `--features pro`. New Pro tests cover clone-aware duplicates with a real `clonefile` clone (the owner's case: a file, a separate copy and a clone give one copy's size as extra), the explorer's Exclusive column measured in the background and its notice for imported scans, the Duplicates view from Enter through Move to Trash, the warning before trashing an Xcode archive or a folder that holds one, the shared `fixtures/cleanup` scan with its review list, `--budget` parsing, text and JSON, each snapshot's date in `explain`, the HTML page against the shared `fixtures/html-report` structure, and on Linux the rule that hides single files mounted into a container.
- `scripts/check-cli.py` checks, with the Pro archive, duplicates' `clone` and `exclusiveBytes` (a hard-linked copy frees nothing), `--cleanup`'s `review` list with an Xcode archive, and `diff --budget` exiting with `3` over budget and `0` within it. `scripts/check-tui.py` checks Pro's `X` column, the `D` view with Enter, marking and the y/N prompt, and the `E` panel.
- All 63 Swift tests passed (42 in SpaceCore and 21 in SpacePro). New ones cover clone-aware duplicate groups with the same folder as the Rust test, whole-scan exclusive sizes with progress, budget parsing and the receipt's budget line with the CLI's rule, the review list from the shared cleanup fixture and its warning, the older-than filter, and the HTML page against the shared structure.
- **Mac runtime QA.** A temporary harness, not committed, drove `ScanWorkspace` in a release build of the app run with `-proFeatures NO` and `YES` argument overrides, logged to standard error, and rendered the real window and its sheets. It confirmed: the start screen's Pro hint with Pro off, and no Pro menu items; with Pro on, **View → Show Exclusive Size** and **Show Files Older Than** with its presets, and **File → Export HTML Report…**; the Exclusive column (a folder holding a file and its clone shows 2.0 MB of 6.0 MB) and its sorting both ways; Duplicates with the owner's case showing 2.0 MB in extra copies and both clones labeled **Clone, frees nothing**; Cleanup listing Archives as review, the Move to Trash alert starting with "Xcode archives can't be recreated." and none for `node_modules`; the age filter keeping only the old file and its folder, refusing to trash the folder, and turning Compare off; the HTML report and receipt save panels with their names and messages, and the receipt's **Over budget** line; the receipt sheet's budget field; and Explain Disk Space listing two snapshots oldest first with the note. It found and this release fixes two layout bugs: after the Exclusive column or Compare's Change column was hidden, the table kept stale widths and overflowed behind a horizontal scroller, cutting off Share, and the hint's link was gray instead of link blue.
- AppKit logs "reentrant operation in its NSTableView delegate" once when the age filter is cleared and once when the Exclusive sort is reversed. Nothing on screen is wrong, and v0.8.0 logs nothing for comparable steps. 0.9.1 fixes it.
- Local release archives (macOS `tar` and `gzip`), Basic and Pro: `aarch64-apple-darwin` 505,519 and 717,383 bytes (0.8.0: 505,118 and 693,055). Linux `x86_64-unknown-linux-musl` archives as `package-cli.sh` builds them: Basic 590,550 bytes against 590,246 before M7, and Pro 834,274 against 808,813. Basic grew by about 300 bytes for the bound-file rule, the explorer's trash prompt and Pro's key hooks, with no new dependencies, so its Linux x86-64 archive keeps about 9.4 KB under the 600 KB budget. Pro grew by about 25 KB with no new dependencies.
- Published release archives, Basic and Pro, against 0.8.0's: `aarch64-apple-darwin` 511,525 and 725,400 bytes (511,027 and 701,274), `x86_64-apple-darwin` 548,946 and 796,410 (548,736 and 767,528), `aarch64-unknown-linux-musl` 557,176 and 774,034 (556,766 and 749,442), and `x86_64-unknown-linux-musl` 590,199 and 835,240 (589,857 and 809,933). Basic grew by 210–500 bytes on every target, and its Linux x86-64 archive has about 9.8 KB left under the 600 KB budget. Publish release run 45012449990 attached all 21 assets, and the release notes are 0.9.0's changelog section.

Rust release CLI, the same day. The 100,000-file row is `scripts/benchmark.py`, the last of three runs. The 1,000,000-file row times the three builds on one fixture, alternating, after a warm-up run of each, with 0.8.0 Pro built from its tag.

| Fixture | 0.8.0 Pro | 0.9.0 Basic | 0.9.0 Pro |
| --- | --- | --- | --- |
| 100,000 files | | 0.0394 s; 18.6 MB | 0.0400 s; 19.5 MB |
| 1,000,000 files | 1.145, 1.134, 1.157, 1.168 s; 128.4–128.5 MB | 1.140, 1.144, 1.136, 1.158 s; 120.1–120.2 MB | 1.139, 1.141, 1.155, 1.154 s; 128.2–128.3 MB |

Both editions scan as fast as 0.8.0 with the same memory. M7 doesn't change the scanner: exclusive sizes and duplicates do their extra reading after the scan, and only when asked.

## 0.8.0

Milestone 6 (Pro deep) on the Apple M5 Pro MacBook Pro with macOS 27.2.1, APFS and Rust 1.100.0.

- The Rust CLI passed 73 unit tests in the Basic edition and 109 with `--features pro`. New Pro tests cover `explain` (snapshot names and dates, APFS containers, the plain and JSON reports, and on Linux a deleted file the test holds open), the shared `fixtures/cleanup` scan and its report, the explorer's `u` view through Move to Trash, exclusive sizes with hard links and, on macOS, a `clonefile` clone, `FIEMAP` on Linux, duplicate groups (a late difference, a hard link, files under the minimum) and size parsing, the HTML page (escaping, no scripts or links), the squarified layout, the explorer's `m` view, and parsing of every new command and option in both editions.
- `scripts/check-cli.py` checks, with the Pro archive, `explain` (a held-open deleted file on Linux), `--cleanup`, `--exclusive` (a hard link everywhere and a `cp -c` clone on macOS), `duplicates` (groups, `--min-size`, a hard link, and an unreadable file giving exit 2) and `--html` (a file and standard output). With the Basic archive, it checks that each new command and option prints the install hint. `scripts/check-tui.py` checks Pro's `u` and `m` views.
- All 55 Swift tests passed (42 in SpaceCore and 13 in SpacePro). New ones read the same `fixtures/cleanup` scan as the Rust tests and cover snapshot names and APFS containers, a real explanation of the temporary folder's volume, exclusive sizes with a hard link and a `clonefile` clone, duplicate groups in the same folder layout as the Rust test, the low-space alert rule, and a Build Footprint receipt across two scans of a changing folder.
- Offscreen renders of the shipping views checked **Explain Disk Space**, **Cleanup** from the shared fixture, **Duplicates** before and after **Find Duplicates**, the receipt sheet and **Settings → Pro features** with the menu-bar options, in light and dark. The packaged app launched with Pro on and quit cleanly. The menu-bar item, the low-space notification, a second window and the receipt's save panel haven't been checked by hand. See the manual QA rows.
- On this Mac, `explain` lists the six mounted volumes of the startup disk's APFS container, from 813.65 GB for the data volume to 24.3 MB for Update, and 10.39 GB of purgeable space. `scan ~/Library --exclusive` found 39.31 GB of caches of which 36.43 GB is exclusive, and took about 2 seconds longer than the 1.8-second scan for 270,000 files. `duplicates ~/work` found 1,091 groups with 14.16 GB in extra copies in 3 seconds with a warm cache.
- Published release archives, Basic and Pro, against 0.7.0's: `aarch64-apple-darwin` 511,027 and 701,274 bytes (513,418 and 613,724), `x86_64-apple-darwin` 548,736 and 767,528 (551,048 and 665,001), `aarch64-unknown-linux-musl` 556,766 and 749,442 (558,807 and 661,990), and `x86_64-unknown-linux-musl` 589,857 and 809,933 (592,316 and 707,438). Basic shrank by about 2–2.5 KB on every target, because its archive no longer carries the Pro reference, and its Linux x86-64 archive has about 10.1 KB left under the 600 KB budget. Publish release run 44939336421 attached all 21 assets.
- The scanner, the tree and Basic's memory are unchanged. Exclusive sizes are measured after the scan and kept only for files that share blocks.
- Local release archives (macOS `tar` and `gzip`), Basic and Pro: `aarch64-apple-darwin` 505,118 and 693,055 bytes (0.7.0: 507,308 and 607,158). Linux `x86_64-unknown-linux-musl` archives as `package-cli.sh` builds them, with `cli.md`, the example report and the license: Basic 590,246 bytes against 592,542 before M6, and Pro 808,813. Basic's binary grew by 416 bytes for the new hints, and moving the Pro reference out of `cli.md` took about 2.7 KB off its archive, so it ends about 2.3 KB smaller with no new dependencies. Pro grew by about 100 KB with no new dependencies, and has no size budget.

Rust release CLI, the same day. The 100,000-file row is `scripts/benchmark.py`, the last of three runs. The 1,000,000-file row times the three builds on one fixture, alternating, after a warm-up run of each, because separate `benchmark.py` runs on freshly created fixtures measured Pro at 1.28–1.44 s once and couldn't be repeated on a shared fixture.

| Fixture | 0.7.0 Pro | 0.8.0 Basic | 0.8.0 Pro |
| --- | --- | --- | --- |
| 100,000 files | | 0.0395 s; 18.6 MB | 0.0399 s; 19.6 MB |
| 1,000,000 files | 1.150, 1.142, 1.153, 1.152 s | 1.136, 1.136, 1.154, 1.157 s; 120.1–120.4 MB | 1.137, 1.141, 1.158, 1.156 s; 128.2–128.4 MB |

Both editions scan as fast as 0.7.0 with the same memory: M6 doesn't change the scanner, and `--exclusive` and `duplicates` do their extra reading only when asked.

## 0.7.0

Milestone 5 (Pro analysis) on the Apple M5 Pro MacBook Pro with macOS 27.2.1, APFS and Rust 1.100.0.

- The Rust CLI passed 73 unit tests in the Basic edition and 90 with `--features pro`. New Pro tests cover modification times on a real scan with 1 and 4 threads (the shared `fixtures/modified-times.json`, with a hard link, a pre-1970 time and empty folders), the details side table through Trash and rescan rebuilds, `--older-than` (kept files, kept protected folders, recomputed totals and times), local and UTC time formatting, the shared `fixtures/file-types.json` table, type ranking and reports, the explorer's `t` panel, owner sums, reports and user lookup, history files, the index (damaged lines, missing files, permissions) and file names, and parsing of every new option in both editions.
- `scripts/check-cli.py` checks, with the Pro archive, `newestModified` against the shared fixture's times, `--older-than` on real file times, `--types`, `--by-owner` against the scan's totals, and `history` (nothing written before a save, listing, permissions, excludes kept, `diff` and `--since`). With the Basic archive, it checks that each Pro option and `history` print the install hint. `scripts/check-tui.py` checks Pro's Modified column and `t` panel.
- All 47 Swift tests passed (42 in SpaceCore and 5 in SpacePro). New ones read the same modification-time and file-type fixtures as the Rust tests, check that `ScanEntry` stays 64 bytes with its modification time, and cover the saved-scan history, including file names that match the CLI's.
- Offscreen renders of the shipping views, in the promo, show the folder list with the **Modified** column and dates, **Types** in the Contents picker, and Compare against a saved scan. The history menus and the Types table haven't been checked by hand. See the manual QA rows.
- Basic's tree nodes stay 40 bytes and its walk records are unchanged, since `Details` has no fields in Basic. Pro keeps 8 bytes per node in the side table.
- Local release archives (macOS `tar` and `gzip`), Basic and Pro: `aarch64-apple-darwin` 507,308 and 607,158 bytes (0.6.0: 504,101 and 527,196). Linux `x86_64-unknown-linux-musl` binaries linked with `rust-lld` and archived alone: Basic 581,621 bytes against 580,071 before M5 (+1.5 KB), and Pro 695,456. The release archive adds about 8 KB of other files, so Basic's Linux x86-64 archive stays near 590 KB, under the 600 KB budget, with no new dependencies. Pro grew by about 70 KB, mostly for reading the history index. Pro has no size budget or dependencies of its own.
- Published release archives, Basic and Pro, against 0.6.0's: `aarch64-apple-darwin` 513,418 and 613,724 bytes (510,178 and 533,482), `x86_64-apple-darwin` 551,048 and 665,001 (548,309 and 576,179), `aarch64-unknown-linux-musl` 558,807 and 661,990 (555,092 and 577,788), and `x86_64-unknown-linux-musl` 592,316 and 707,438 (588,111 and 616,109). Basic grew 3–4 KB and stays under the 600 KB budget on every target. Its Linux x86-64 archive has about 7.7 KB left.

Rust release CLI, `scripts/benchmark.py`, the same day:

| Fixture | 0.6.0 | 0.7.0 Basic | 0.7.0 Pro |
| --- | --- | --- | --- |
| 100,000 files | 0.0406, 0.0395, 0.0412 s; 18.7 MB | 0.0399, 0.0396, 0.0394 s; 18.6–18.7 MB | 0.0406, 0.0398, 0.0396 s; 19.5 MB |
| 1,000,000 files | 1.165, 1.155, 1.155 s; 120.1–120.2 MB | 1.155, 1.149, 1.151 s; 120.1–120.2 MB | 1.164, 1.146, 1.138 s; 128.3–128.5 MB |

Basic scans as fast as 0.6.0, with the same memory. Pro reads modification times and owners with the same calls, so it's as fast, and its side table adds about 8 bytes per entry, 8 MB at 1,000,000 files.

## 0.6.0

Milestone 4 (Carry, compare, editions) on the Apple M5 Pro MacBook Pro with macOS 27.2.1, APFS and Rust 1.100.0.

- The Rust CLI passed 73 unit tests in the Basic edition and 77 with `--features pro`. New tests cover ncdu export and import (a round trip with states, issues, excludes and non-UTF-8 names; ncdu and gdu files with hard links, exclusions, special files, read errors, escapes and unknown keys; malformed files), the explorer's read-only imported mode, `volumes` (rounding, the table, JSON and the root volume), edition routing, and `diff` (depth listings, top files, signed totals). The Swift and Rust importers read the same files in `fixtures/scan-exports/` and agree on every entry's sizes, item counts and flags, every issue, and the excluded count.
- `scripts/check-cli.py` covers `--export` to a file and to standard output, `--import` from a file and a pipe, partial exports, `volumes`, the Basic hint for Pro commands, and with the Pro archive, `diff`. On the Linux CI runners, ncdu 1.19 and gdu open our export, and we open theirs with the same totals as our own scan. `scripts/check-tui.py` drives an imported scan from a file and from a pipe in a pseudo-terminal. `scripts/check-install.py` switches between the Basic and Pro archives.
- All 42 Swift tests passed. They include the shared export fixtures, a round trip of a real scan, volume summaries, and Pro's comparison below different roots.
- Offscreen renders of the shipping views checked the start screen with the volume list and **Open a Saved Scan…**, an opened scan with the read-only label, **Settings → Pro features**, and Compare with its Change column ranked by growth.
- 1,000,000 files exported in a 50 MB file. Scanning and exporting took 1.23 s, and importing took 0.23 s with a 33 MB peak, because the reader streams.
- Published release archives, Basic and Pro, against 0.5.0's Basic archives: `aarch64-apple-darwin` 510,178 and 533,482 bytes (532,615), `x86_64-apple-darwin` 548,309 and 576,179 (566,289), `aarch64-unknown-linux-musl` 555,092 and 577,788 (555,482), and `x86_64-unknown-linux-musl` 588,111 and 616,109 (588,488). Basic stays under the 600 KB budget on every target.
- Local release archives (macOS `tar` and `gzip`), Basic and Pro: `aarch64-apple-darwin` 504,101 and 527,196 bytes, and `x86_64-unknown-linux-musl` 588,977 and 616,395. 0.5.0's published `aarch64-apple-darwin` archive was 532,615 bytes. Building with full LTO and one codegen unit took about 28 KB off each archive before the new features, so Basic stays under the 600 KB budget with export, import and `volumes`, and with no new dependencies. Pro has no size budget and no dependencies of its own yet.

Rust release CLI, `scripts/benchmark.py`, the same day:

| Fixture | 0.5.0 | 0.6.0 Basic |
| --- | --- | --- |
| 100,000 files | 0.0415, 0.0393, 0.0392 s; 18.7–18.8 MB | 0.0428, 0.0404, 0.0404 s; 18.5–18.6 MB |
| 1,000,000 files | 1.148, 1.147, 1.145 s; 120.2–120.3 MB | 1.146, 1.138, 1.134 s; 120.0–120.2 MB |

Scans are as fast as in 0.5.0 and use the same memory.

## 0.5.0

Milestone 3 (Speed) on the Apple M5 Pro MacBook Pro with macOS 27.2.1, APFS and Rust 1.100.0.

- The Rust CLI passed 62 unit tests on macOS. New tests cover the arena tree (layout, bytewise path order without building paths, removal, replacement and overflow), a 200,000-level record chain, and determinism: 2, 3 and 8 threads, four runs each, against one thread, with hard links across folder groups, an excluded pattern and unreadable folders. Others cover threaded previews and cancellation, and the macOS bulk listing against `fstatat`. `scripts/check-cli.py` compares JSON output for `--threads 1, 2, 8, 64` and rejects invalid counts. `scripts/check-tui.py` passes.
- Before the CLI used `getattrlistbulk`, a standalone check compared its attributes with `fstatat` for every entry under `/Applications`, `~/Library/Developer`, `/System/Library/Frameworks` and `/usr`. All 590,000 files matched in allocated bytes, lengths, inode numbers, link counts, devices and flags, including compressed apps. Only firmlinked folders and a mount point differed, and folders still use `fstatat`.
- All 34 Swift tests passed, including scans at concurrency 2, 3 and 8 that match a single directory at a time, and a check that every directory task reads with cloud downloads off.
- Published release archives, against 0.4.0: `aarch64-apple-darwin` 532,615 bytes (503,142), `x86_64-apple-darwin` 566,289 (536,346), `aarch64-unknown-linux-musl` 555,482 (524,524), and `x86_64-unknown-linux-musl` 588,488 (555,661). All stay under the 600 KB budget, with no new dependencies. Passing the walk's callbacks as trait objects took about 6 KB off the Linux x86-64 archive before release.

Rust release CLI, `scripts/benchmark.py` with one warm-up run before each set of three. 0.4.0 was measured the same day.

| Fixture | 0.4.0 | 0.5.0 (8 threads) |
| --- | --- | --- |
| 100,000 files | 0.2039, 0.2044, 0.1966 s; 40.2–40.3 MB | 0.0509, 0.0399, 0.0393 s; 18.4–18.5 MB |
| 1,000,000 files | 5.658, 5.492, 5.531 s; 342.2–342.3 MB | 1.265, 1.236, 1.240 s; 118.9–119.1 MB |

At 1,000,000 files the scan is 4.4 times faster and peak memory falls by 65%, about 119 bytes per entry instead of 342. Three changes contribute:
- **Arena tree.** Name-only nodes alone cut memory to 121 MB, while one thread still took 5.3 s.
- **Threads.** Eight threads brought it to 2.5 s, but system time grew from 4.3 s to 18.6 s: with 1,000,000 files, each `fstatat` recycled a vnode (`kern.maxvnodes` is 263,168), and threads contended in the kernel.
- **Bulk reads.** Reading each folder's file metadata with `getattrlistbulk` removed most of that contention.

Four threads were slightly faster than eight on this machine (1.18 s against 1.37 s on a reused fixture). The default stays at one thread per core, up to 8.

The Mac app's scanner, measured with a harness that links `SpaceCore` on the same fixtures:

| Fixture | 0.4.0 | 0.5.0 |
| --- | --- | --- |
| 100,000 files | 0.19 s, 43 MB | 0.08 s, 21 MB |
| 1,000,000 files | 5.6 s, 379 MB | 2.1–2.5 s, 132 MB |

## 0.3.0

- The Rust CLI passed 31 unit tests, including the shared exclude-pattern table in `fixtures/exclude-patterns.json`, scanner tests for apparent bytes, item counts and exclusions, and explorer tests for sorting and the size toggle. `scripts/check-cli.py` covers JSON schema 3, `--apparent` ordering, the hard-link rule for items, and `--exclude` / `--exclude-from`. `scripts/check-tui.py` drives `s` and `a` in a pseudo-terminal and checks that `NO_COLOR` and `--no-color` emit no color codes while keeping the selection in reverse video.
- All 25 Swift tests passed. The Swift matcher reads the same pattern table, and the Swift excludes test mirrors the Rust one with the same tree, patterns and totals: 4 excluded, 3 items, 17 bytes.
- Offscreen renders of the Mac app checked the sortable list, the Items column, apparent sizes in the list and treemap, the excluded count, and the Settings pane in dark mode. The promo Short rendered at 1080 × 1920 and 29.5 seconds.
- The packaged `aarch64-apple-darwin` CLI archive is 450,872 bytes, compared with 441,692 bytes for 0.2.4.

The Rust release CLI benchmark ran on the Apple M5 Pro MacBook Pro with macOS 27.2.1 and APFS, with one warm-up run before each set.

| Fixture | Wall time (3 runs) | Peak resident memory |
| --- | --- | --- |
| 100,000 files (`scripts/benchmark.py`) | 0.1581, 0.1567, 0.1653 s | 36,241,408–36,274,176 bytes |
| 1,000,000 files (`--files 1000000`) | 5.533, 5.450, 5.423 s | 338,198,528–338,296,832 bytes |

Peak memory at 100,000 files rose about 5% from 0.2.0's 34.5 MB, because each entry now also records apparent bytes and an item count. Memory grows linearly with the file count, while time grows faster than linearly. Milestone M3 targets both with a leaner tree and a parallel walk.

## 0.2.0

- The Rust CLI passed 14 unit tests on Apple silicon and 13 each on Linux ARM64 and x86-64. The release archives for those three targets and Intel macOS passed the CLI and pseudo-terminal checks. Intel macOS ran through Rosetta. The terminal checks include directory navigation, largest files, resize, and terminal restoration after quit and Ctrl-C.
- The Linux archives passed static-linkage and SHA-256 checks. Both Mac executables passed ad hoc signature verification and declare macOS 14 as their minimum. All four archives include the MIT license.
- The installer fixture exercises curl and GNU Wget, all four platform routes, version selection, missing assets, checksum rejection, and refusal to follow HTTP redirects. GitHub installation uses the published release archives.
- All 12 Swift tests passed. The Rust CLI also scanned the previously reported Books package on this Mac in 0.056 seconds with its cloud-related filesystem flags unchanged.

The Rust release CLI benchmark used the 100,000-file fixture on the Apple M5 Pro MacBook Pro with macOS 27.2.1 and APFS. The three runs took 0.1893, 0.1848, and 0.1978 seconds. Peak resident memory ranged from 34,521,088 to 34,553,856 bytes. See [benchmark instructions](../CONTRIBUTING.md#benchmark-a-scanner-change).

## 0.1.1

- Twelve macOS fixture tests and eleven Ubuntu x86-64 fixture tests passed. The new tests cover the depth cutoff, completed-folder updates, cancellation, hard-link attribution across the preview boundary, and macOS's bounded provider calls.
- The native-window harness preserved navigation into a pending folder when deeper results arrived. It also kept the preview browseable after cancellation. The donut and legend were inspected at narrow and wide widths in light and dark appearances.
- The reported Books package originally stalled inside a directory open, even with dataless materialization disabled. After adding the deadline, the package and its Books directory completed a local retest in approximately 0.05 and 0.16 seconds, respectively, with their flags unchanged. A controlled test exercised the one-second timeout and verified that the same stalled path was not retried during the scan.

## 0.1.0

Local run on 18 November 2026. macOS testing used a MacBook Pro with an Apple M5 Pro, 48 GB of memory, macOS 27.2.1, and Xcode 27.2. Ubuntu 24.04 x86-64 ran in Docker Desktop on the same Mac.

| Check | Result |
| --- | --- |
| macOS scanner | Seven fixture tests passed, including sparse files, ties, hard links, symlinks, permissions, Unicode paths, progress, and cancellation. |
| Linux scanner | Eight fixture tests passed as an unprivileged user. The Linux suite also checked undecodable POSIX filenames. |
| CLI | The macOS acceptance script and the packaged Ubuntu executable passed checks for both ranking metrics and scan exit statuses. The Ubuntu package ran offline on a clean system without Swift and skipped a real mount from a different device. |
| Desktop and CLI agreement | A nested file ranked first by allocated size, while a sparse file ranked first by logical size. Both interfaces reported 4,198,400 allocated bytes and 71,303,191 logical bytes for the same APFS fixture. |
| Desktop views | The packaged app launched. A local harness exercised the native folder list at its minimum width, the treemap, the largest-files table, and the partial-results sheet. It also verified navigation, cancellation, and recovery in the workspace model. |
| Package integrity | The app passed strict ad-hoc signature verification. Both Mach-O slices declare macOS 14.0 as the minimum. The Ubuntu archive passed SHA-256 verification. |

## Swift CLI benchmark (0.1.0)

The release CLI scanned 100,000 files of 512 logical bytes each in 1,100 subdirectories on the Mac’s APFS data volume. Each timed run requested the 20 largest files by allocated size with JSON output. The harness generated the fixture and performed one warm-up before timing. Wall time includes starting the process and writing the report.

| Run | Seconds | Peak resident bytes |
| --- | ---: | ---: |
| 1 | 0.2193 | 34,127,872 |
| 2 | 0.2127 | 34,111,488 |
| 3 | 0.2123 | 34,095,104 |

The current Rust CLI can run the same fixture via `python3 scripts/benchmark.py PATH_TO_RELEASE_CLI`. Storage, directory shape, cache state, and other activity affect the result.

## Public release checks

The remaining checks cover physical Finder input and reveal, VoiceOver, mounted volumes on macOS, launch on macOS 14 and a physical Intel Mac, hosted CI, and Developer ID signing and notarization. See the [manual macOS checklist](manual-qa.md) and [release procedure](release.md).
