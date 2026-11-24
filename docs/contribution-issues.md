# Contribution issue drafts

## Verify DiskUse16x on macOS 14

Run the [manual macOS checks](manual-qa.md) on macOS 14. Use a synthetic fixture and the candidate universal build. Record the exact OS version and architecture, note any failing actions, and include a reproducible example for each failure. This helps establish the project's macOS support floor.

## Record the scanner benchmark on Linux

Build the CLI with the pinned Rust toolchain on a named x86-64 or ARM64 machine and run the [benchmark](../CONTRIBUTING.md#benchmark-a-scanner-change). Record the CPU, operating system, Rust version, filesystem, elapsed times, and peak resident memory. Keep the default fixture and warm-up so the numbers can be compared with later changes on the same host.

## Verify metadata-only scans on a removable drive

Use a test folder on a removable drive with known file sizes, including a sparse file if the filesystem supports it. Compare desktop and CLI allocated totals and report the filesystem and connection type. Confirm that selecting the mounted drive directly counts its readable regular files.
