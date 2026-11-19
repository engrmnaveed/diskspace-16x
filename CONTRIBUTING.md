# Contributing to Space 16X

Open an issue with the exact command or app action, your OS and filesystem, and the results you expected. Use a small synthetic folder to reproduce scan issues. Remove local paths and names before sharing reports or screenshots.

Report security vulnerabilities privately as described in [SECURITY.md](SECURITY.md), not in a public issue.

## Development

The CLI uses Rust and the toolchain pinned in [cli/rust-toolchain.toml](cli/rust-toolchain.toml). The native app uses Swift 6 or later and builds on macOS.

| Path | Role |
| --- | --- |
| `cli/` | Terminal UI, scriptable reports, and portable metadata scanner. |
| `Sources/SpaceCore/` | Swift scanner, scan previews, and filesystem accounting for the app. |
| `Sources/SpaceApp/` | Native app with SwiftUI and AppKit. |

Run the CLI checks from the repository root.

```sh
(cd cli && cargo fmt --all --check && cargo clippy --locked --all-targets -- -D warnings)
(cd cli && cargo test --locked && cargo build --locked)
python3 scripts/check-cli.py cli/target/debug/space16x
python3 scripts/check-tui.py cli/target/debug/space16x
python3 scripts/check-install.py
```

Run permission fixtures as a standard user. The tests use temporary directories and synthetic files. For filesystem accounting changes, compare full scans of the same stable fixture on macOS and Linux. Include a focused fixture when the current tests would miss a regression.

On macOS, run `swift test` and `open "$(scripts/package-macos.sh)"`. Exercise folder selection and drop, browsing, Finder reveal, clipboard, partial-scan details, and cancellation. Use the menu and keyboard to repeat the core flows. See the [manual checklist](docs/manual-qa.md).

## Benchmark a scanner change

```sh
(cd cli && cargo build --locked --release)
python3 scripts/benchmark.py cli/target/release/space16x
```

The benchmark creates 100,000 files with a 512-byte payload under 1,100 directories. It runs one warm-up and three timed CLI scans, verifies the allocated total against filesystem metadata, and reports wall time and peak resident memory. The fixture is temporary and requires a few hundred MiB of local disk space on filesystems with 4 KiB blocks. Use `--fixture-parent PATH` to compare filesystems and record the toolchain version with the results.

Keep changes focused. Use commit messages that explain the outcome, and include the verification commands and platform coverage in your pull request.

For packaging and publication, see the [release runbook](docs/release.md).
