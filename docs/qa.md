# Space 16X verification

## 0.5.0

Milestone 3 (Speed) on the Apple M5 Pro MacBook Pro with macOS 27.2.1, APFS and Rust 1.100.0.

- The Rust CLI passed 62 unit tests on macOS. New tests cover the arena tree (layout, bytewise path order without building paths, removal, replacement and overflow), a 200,000-level record chain, and determinism: 2, 3 and 8 threads, four runs each, against one thread, with hard links across folder groups, an excluded pattern and unreadable folders. Others cover threaded previews and cancellation, and the macOS bulk listing against `fstatat`. `scripts/check-cli.py` compares JSON output for `--threads 1, 2, 8, 64` and rejects invalid counts. `scripts/check-tui.py` passes.
- Before the CLI used `getattrlistbulk`, a standalone check compared its attributes with `fstatat` for every entry under `/Applications`, `~/Library/Developer`, `/System/Library/Frameworks` and `/usr`. All 590,000 files matched in allocated bytes, lengths, inode numbers, link counts, devices and flags, including compressed apps. Only firmlinked folders and a mount point differed, and folders still use `fstatat`.
- All 34 Swift tests passed, including scans at concurrency 2, 3 and 8 that match a single directory at a time, and a check that every directory task reads with cloud downloads off.
- Release archives: `aarch64-apple-darwin` is 525,888 bytes, up from 503,142 for 0.4.0. A local `x86_64-unknown-linux-musl` build packs to 588,710 bytes, against 555,381 for 0.4.0 built the same way (its published archive is 555,661 bytes). Both stay under the 600 KB budget, with no new dependencies.

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
