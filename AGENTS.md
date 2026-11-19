# Working on Space 16X

- `Sources/SpaceApp` and `Sources/SpaceCore` implement the macOS app in Swift. `cli/` is the Rust CLI for macOS and Linux. Cargo uses the pinned toolchain in `cli/rust-toolchain.toml`.
- Keep filesystem operations read-only and metadata-only. Maintain allocated-byte accounting, hard-link deduplication, symlink and device boundaries, and explicit partial results. Use synthetic fixtures to exercise changes on both scanners when shared behavior is affected.
- Preserve the versioned CLI report and exit-status contract in [docs/cli.md](docs/cli.md). Synchronize CLI and bundle release versions through `cli/Cargo.toml` and `scripts/release-version`.
- For Rust changes, run `(cd cli && cargo fmt --all --check && cargo clippy --locked --all-targets -- -D warnings && cargo test --locked)`. Build the CLI and run `scripts/check-cli.py` and `scripts/check-tui.py` with its executable. Run `python3 scripts/check-install.py` when editing the installer.
- For Swift changes, run `swift test` and `scripts/package-macos.sh` on macOS. Use the [manual checklist](docs/manual-qa.md) for UI behavior.
- Run permission checks as a standard user. Keep signing, credentials, and publication in the [release workflow](docs/release.md).
