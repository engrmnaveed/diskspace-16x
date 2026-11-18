# Contribution issue drafts

## Verify Space 16X on macOS 14

Run the [manual macOS checks](manual-qa.md) on macOS 14. Use a synthetic fixture and the candidate universal build. Record the exact OS version and architecture, note any failing actions, and include a reproducible example for each failure. This helps establish the project's macOS support floor.

## Record the scanner benchmark on Ubuntu 24.04 x86-64

Build the CLI with Swift 6.2.4 on a named machine and run the [benchmark](../CONTRIBUTING.md#benchmark-a-scanner-change). Record the CPU, operating system, Swift version, filesystem, elapsed times, and peak resident memory. Keep the default fixture and warm-up so the numbers can be compared with later changes on the same host.

## Verify metadata-only scans on a removable drive

Use a test folder on a removable drive with known file sizes, including a sparse file if the filesystem supports it. Compare desktop and CLI totals for both metrics and report the filesystem and connection type. Confirm that selecting the mounted drive directly counts its readable regular files.
