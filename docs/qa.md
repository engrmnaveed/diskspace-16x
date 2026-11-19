# Space 16X verification

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
