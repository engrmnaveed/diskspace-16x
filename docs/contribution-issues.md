# Contribution issues

These were posted on 2026-11-24, labelled **help wanted** and, where no setup is needed, **good first issue**:

- [#93 Verify DiskUse16x on macOS 14](https://github.com/engrmnaveed/diskuse16x/issues/93)
- [#94 Record the scanner benchmark on a real Linux machine](https://github.com/engrmnaveed/diskuse16x/issues/94)
- [#95 Verify metadata-only scans on a removable drive](https://github.com/engrmnaveed/diskuse16x/issues/95)
- [#96 Check DiskUse16x on an Intel Mac](https://github.com/engrmnaveed/diskuse16x/issues/96)
- [#97 Try the CLI on a real Linux desktop or server](https://github.com/engrmnaveed/diskuse16x/issues/97)

The drafts they started from follow. Add new drafts here before posting them.

## Verify DiskUse16x on macOS 14

Run the [manual macOS checks](manual-qa.md) on macOS 14. Use a synthetic fixture and the candidate universal build. Record the exact OS version and architecture, note any failing actions, and include a reproducible example for each failure. This helps establish the project's macOS support floor.

## Record the scanner benchmark on Linux

Build the CLI with the pinned Rust toolchain on a named x86-64 or ARM64 machine and run the [benchmark](../CONTRIBUTING.md#benchmark-a-scanner-change). Record the CPU, operating system, Rust version, filesystem, elapsed times, and peak resident memory. Keep the default fixture and warm-up so the numbers can be compared with later changes on the same host.

## Verify metadata-only scans on a removable drive

Use a test folder on a removable drive with known file sizes, including a sparse file if the filesystem supports it. Compare desktop and CLI allocated totals and report the filesystem and connection type. Confirm that selecting the mounted drive directly counts its readable regular files.

## Check DiskUse16x on an Intel Mac

Install the latest Mac app and CLI on a physical Intel Mac and run the core rows of the [manual macOS checks](manual-qa.md) with its synthetic test folder. Record the Mac's model and CPU, the macOS version, and which checks passed.

## Try the CLI on a real Linux desktop or server

Install the CLI with the one-liner, a package or Homebrew on a real Linux machine. Explore a folder, try `volumes` and `scan --top`, and in a desktop session move a disposable file to the Trash and restore it from the file manager. Optionally run `scripts/check-cli.py` and `scripts/check-tui.py` against the installed command.
