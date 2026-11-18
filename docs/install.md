# Install and run Space 16X

## Build from source

Use Swift 6 or later with a complete toolchain for your platform. Read permission is sufficient for ordinary local folders. On macOS, the app uses the system folder picker and standard Finder actions.

### macOS

The deployment target is macOS 14. Run these commands from the checkout on a Mac with Xcode or the Xcode Command Line Tools installed:

```sh
scripts/package-macos.sh
open "dist/Space 16X.app"
```

The app in `dist` is built for the current Mac and ad hoc signed for local development. Use `scripts/package-macos.sh --universal` to build both arm64 and x86-64 slices. If you want the command line tool, install it separately:

```sh
swift build -c release --product space16x
mkdir -p "$HOME/.local/bin"
install -m 0755 "$(swift build -c release --show-bin-path)/space16x" "$HOME/.local/bin/space16x"
```

### Linux

The first release target is Ubuntu 24.04 x86-64. With the Swift toolchain installed:

```sh
swift build -c release --product space16x
mkdir -p "$HOME/.local/bin"
install -m 0755 "$(swift build -c release --show-bin-path)/space16x" "$HOME/.local/bin/space16x"
```

Add `$HOME/.local/bin` to your shell's `PATH` to use the installed command by name. A default SwiftPM build on Linux uses runtime libraries from the build toolchain. For a transferable Ubuntu package, use `scripts/package-linux.sh` to link the Swift runtime statically and produce a checksummed archive.

## Install a release bundle

Use the following steps once release bundles are available.

### macOS release

Download the universal ZIP and its companion SHA-256 file. From the download directory, verify and expand the ZIP:

```sh
shasum -a 256 -c space16x-0.1.0-macos-universal.zip.sha256
ditto -x -k space16x-0.1.0-macos-universal.zip .
```

Move **Space 16X.app** to Applications in Finder and open it. The release app uses Developer ID signing and Apple notarization.

### Ubuntu release

Download the CLI archive and its companion SHA-256 file. From the download directory:

```sh
sha256sum -c space16x-0.1.0-linux-x86_64.tar.gz.sha256
tar -xzf space16x-0.1.0-linux-x86_64.tar.gz
mkdir -p "$HOME/.local/bin"
install -m 0755 space16x-0.1.0-linux-x86_64/space16x "$HOME/.local/bin/space16x"
"$HOME/.local/bin/space16x" --version
```

## Mac keyboard shortcuts

Focus the file table to use arrow keys for row selection. Core commands are available in the menu bar.

| Action | Shortcut |
| --- | --- |
| Choose a folder or local drive | ⌘O |
| Scan the same location again | ⌘R |
| Cancel a scan | ⌘. |
| Browse folders | ⌘1 |
| Show largest files | ⌘2 |
| Show the folder treemap | ⌘3 |
| Open a selected folder | ⌘↓ |
| Go to the parent folder | ⌘↑ |
| Reveal a selected item in Finder | ⇧⌘R |
| Copy a selected path | ⌥⌘C |
| Show scan details | ⌘I |

If access to a folder is restricted, open **Scan Details** to see any skipped paths, or scan a location you can read. The app and CLI keep scan data in memory. Redirect CLI output to save a report intentionally.
