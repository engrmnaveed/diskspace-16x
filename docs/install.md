# Install Space 16X

## Install the CLI from a release

Once the [GitHub Releases page](https://github.com/engrmnaveed/diskspace-16x/releases) has CLI archives and checksums, download and run the installer.

```sh
curl -fsSL https://raw.githubusercontent.com/engrmnaveed/diskspace-16x/main/scripts/install.sh -o space16x-install.sh
sh ./space16x-install.sh
"$HOME/.local/bin/space16x" --version
```

With GNU Wget, download the same installer with this command and run it as above.

```sh
wget -qO space16x-install.sh https://raw.githubusercontent.com/engrmnaveed/diskspace-16x/main/scripts/install.sh
```

The installer selects the target for your OS and architecture, checks the SHA-256 digest, and installs to `$HOME/.local/bin` without sudo. It uses curl or GNU Wget and `sha256sum` or `shasum`. On a minimal Linux image with BusyBox wget, use curl or download and verify the archive manually. Add `$HOME/.local/bin` to your shell's `PATH` to run `space16x` by name.

Use `sh ./space16x-install.sh --version 0.2.0` to choose a published version. Add `--bin-dir PATH` to install elsewhere or `--repo OWNER/REPO` to use a public mirror with the same asset layout. If GitHub returns a missing-asset error, check that the release includes your target or build from source.

### Candidate CLI targets

| System | Release target |
| --- | --- |
| macOS 14 or later, Apple silicon | `aarch64-apple-darwin` |
| macOS 14 or later, Intel | `x86_64-apple-darwin` |
| Linux, ARM64 | `aarch64-unknown-linux-musl` |
| Linux, x86-64 | `x86_64-unknown-linux-musl` |

Linux archives link musl statically and are built and tested on their respective Ubuntu 24.04 runners when hosted CI runs. Other Linux CPU architectures can try a native source build.

### Install a CLI archive manually

Download `space16x-<target>.tar.gz` and its companion `.sha256` file from the same release. The commands below use the Apple silicon archive.

```sh
archive=space16x-aarch64-apple-darwin.tar.gz
shasum -a 256 -c "$archive.sha256"
tar -xzf "$archive" space16x
mkdir -p "$HOME/.local/bin"
install -m 0755 space16x "$HOME/.local/bin/space16x"
```

On Linux, use `sha256sum -c "$archive.sha256"` and substitute your target. The [CLI reference](cli.md) has terminal key bindings and report examples.

## Build the CLI from source

Install [Rustup](https://rustup.rs/) and a native linker for your system. Run the build from a checkout of the repository.

```sh
(cd cli && cargo install --locked --path . --root "$HOME/.local")
"$HOME/.local/bin/space16x" --help
```

The toolchain file under `cli/` selects the project compiler. To make a distributable package, use [the packaging instructions](release.md#package-the-cli-locally).

## Build the Mac app from source

The Mac app targets macOS 14. Build from the checkout on a Mac with Swift 6 or later and the Xcode Command Line Tools.

```sh
scripts/package-macos.sh
open "dist/Space 16X.app"
```

The app in `dist` is built for the current Mac and ad hoc signed for local development. Use `scripts/package-macos.sh --universal` to build both arm64 and x86-64 slices.

## Install a Mac app release

When the signed and notarized app release is available, download its universal ZIP and SHA-256 file. Verify and expand the archive from the download directory.

```sh
shasum -a 256 -c space16x-0.2.0-macos-universal.zip.sha256
ditto -x -k space16x-0.2.0-macos-universal.zip .
```

Move **Space 16X.app** to Applications in Finder and open it.

## Mac app keyboard shortcuts

Focus the file table to use arrow keys for row selection. Core commands are available in the menu bar.

| Action | Shortcut |
| --- | --- |
| Choose a folder or local drive | ⌘O |
| Scan the same location again | ⌘R |
| Cancel a scan | ⌘. |
| Browse folders | ⌘1 |
| Show largest files | ⌘2 |
| Show the folder treemap | ⌘3 |
| Show the folder donut chart | ⌘4 |
| Open a selected folder | ⌘↓ |
| Go to the parent folder | ⌘↑ |
| Reveal a selected item in Finder | ⇧⌘R |
| Copy a selected path | ⌥⌘C |
| Show scan details | ⌘I |

If access to a folder is restricted, open **Scan Details** to see any skipped paths, or scan a location you can read. The app and CLI keep scan data in memory. Redirect CLI output to save a report intentionally.
