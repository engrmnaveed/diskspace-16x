# Contributing to Space 16X

Open an issue with the exact command or app action, your OS and filesystem, and the results you expected. Use a small synthetic folder to reproduce scan issues. Remove local paths and names before sharing reports or screenshots.

## Development

Use Swift 6 or later. The package has no third-party Swift dependencies.

| Target | Role |
| --- | --- |
| `SpaceCore` | Shared metadata traversal, aggregation, ranking, and scan reports. |
| `space16x` | Terminal interface and versioned JSON output. |
| `Space16X` | Native app, built on macOS with SwiftUI and AppKit. |

From the checkout, run the core fixtures and CLI checks before opening a pull request:

```sh
swift test
swift build --product space16x
python3 scripts/check-cli.py "$(swift build --show-bin-path)/space16x"
```

Run permission fixtures as a standard user. The tests use temporary directories and synthetic files. For scanner changes, cover the platform behavior on macOS and Ubuntu 24.04 x86-64. Include a focused regression fixture when the current tests would miss a bug.

On macOS, run `open "$(scripts/package-macos.sh)"` and exercise folder selection and drop, browsing, both metrics, Finder reveal, the clipboard, partial-scan details, and cancellation. Use the menu and keyboard to repeat the core flows. See the [manual checklist](docs/manual-qa.md).

## Benchmark a scanner change

```sh
swift build -c release --product space16x
python3 scripts/benchmark.py "$(swift build -c release --show-bin-path)/space16x"
```

The benchmark creates 100,000 files with a 512-byte payload under 1,100 directories. It runs one warm-up and three timed CLI scans, verifies the logical total, and reports wall time and peak resident memory. The fixture is temporary and requires a few hundred MiB of local disk space on filesystems with 4 KiB blocks. Use `--fixture-parent PATH` to compare filesystems and report the Swift version with the results.

Keep changes focused. Use commit messages that explain the outcome, and include the verification commands and any platform limits in your pull request.

For packaging and publication, see the [release runbook](docs/release.md).
