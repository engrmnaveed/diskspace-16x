# Install Space 16X

## Install the CLI from a release

In Bash, Zsh, or a POSIX shell, paste either one-liner.

```sh
space16x_path=$(curl --proto '=https' --tlsv1.2 -fsSL https://raw.githubusercontent.com/engrmnaveed/diskspace-16x/f36f06107316d5a0312f9c52c4a4e96f2cb34597/scripts/install.sh | sh -s -- --shell-path) && [ -n "$space16x_path" ] && export PATH="$space16x_path" && unset space16x_path
```

```sh
space16x_path=$(wget -qO- https://raw.githubusercontent.com/engrmnaveed/diskspace-16x/f36f06107316d5a0312f9c52c4a4e96f2cb34597/scripts/install.sh | sh -s -- --shell-path) && [ -n "$space16x_path" ] && export PATH="$space16x_path" && unset space16x_path
```

The one-liners download the installer from a fixed commit. To review the script first, download it from the URL above, read it, and run `sh install.sh`. Releases from 0.2.2 also provide an attested `install.sh` alongside the archives on the [releases page](https://github.com/engrmnaveed/diskspace-16x/releases).

Run `space16x --version` or open the explorer with `space16x`. The installer checks the archive's SHA-256 digest and installs to `$HOME/.local/share/space16x/bin`. It sets up your shell's startup file when needed, and the one-liner updates the current shell's PATH. Use curl or GNU Wget for the asset download. On a BusyBox-only image, install curl or use the manual archive instructions.

Pass `--version 0.2.1` after `--shell-path` to select a published version. Use `--bin-dir PATH` to choose another directory or `--repo OWNER/REPO` for a public mirror with the same asset names. [GitHub Releases](https://github.com/engrmnaveed/diskspace-16x/releases)

## Uninstall the managed CLI

Paste either line to remove the executable, installer files, and shell startup entries created by the installer. The command updates PATH in the current shell. It preserves pre-existing PATH entries and manually installed executables.

```sh
space16x_path=$(curl --proto '=https' --tlsv1.2 -fsSL https://raw.githubusercontent.com/engrmnaveed/diskspace-16x/f36f06107316d5a0312f9c52c4a4e96f2cb34597/scripts/install.sh | sh -s -- --uninstall --shell-path) && [ -n "$space16x_path" ] && export PATH="$space16x_path" && unset space16x_path
```

```sh
space16x_path=$(wget -qO- https://raw.githubusercontent.com/engrmnaveed/diskspace-16x/f36f06107316d5a0312f9c52c4a4e96f2cb34597/scripts/install.sh | sh -s -- --uninstall --shell-path) && [ -n "$space16x_path" ] && export PATH="$space16x_path" && unset space16x_path
```

## Fish shell

Use Fish's list syntax to apply PATH immediately. Add `--uninstall` before `--shell-path` to remove the installation. The download can also use `wget -qO-`.

```fish
set -l space16x_path (curl --proto '=https' --tlsv1.2 -fsSL https://raw.githubusercontent.com/engrmnaveed/diskspace-16x/f36f06107316d5a0312f9c52c4a4e96f2cb34597/scripts/install.sh | sh -s -- --shell-path); and test -n "$space16x_path"; and set -gx PATH (string split : -- "$space16x_path")
```

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

### Verify a release archive

The SHA-256 file only proves the download wasn't corrupted, because it comes from the same place as the archive. Each release, starting with 0.2.2, also publishes a GitHub build-provenance attestation for every archive and for `install.sh`. The attestation shows that the file was built by this repository's release workflow from the tagged commit. Check it with the [GitHub CLI](https://cli.github.com/):

```sh
gh attestation verify space16x-aarch64-apple-darwin.tar.gz \
  --repo engrmnaveed/diskspace-16x \
  --signer-workflow engrmnaveed/diskspace-16x/.github/workflows/publish-cli.yml
```

Use the same command with `install.sh` to verify the installer. Add `--source-ref` with the full tag ref, such as `refs/tags/vX.Y.Z`, to require a specific release tag.

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

## Install the Mac app

Releases from 0.2.2 publish a universal Mac app for Apple silicon and Intel on macOS 14 or later. The app is ad hoc signed and is not notarized by Apple, because the project doesn't use a paid Apple Developer account. macOS blocks the first launch until you allow it once.

### Install with Homebrew

The cask lives in the project's tap. Add the tap, then install the app. Homebrew 6 and later only load casks from taps you trust, so run `brew trust` once before installing.

```sh
brew tap engrmnaveed/tap
```

```sh
brew trust engrmnaveed/tap
```

```sh
brew install --cask engrmnaveed/tap/space16x
```

Upgrade with `brew upgrade --cask space16x`. Remove the app with `brew uninstall --cask space16x`. Add `--zap` to that command to also remove its preferences and saved window state.

### Install the release ZIP

Download `space16x-<version>-macos-universal.zip` and its `.sha256` file from the [releases page](https://github.com/engrmnaveed/diskspace-16x/releases). Verify the archive and expand it from the download directory. The commands below use version 0.2.4 as an example.

```sh
shasum -a 256 -c space16x-0.2.4-macos-universal.zip.sha256
gh attestation verify space16x-0.2.4-macos-universal.zip \
  --repo engrmnaveed/diskspace-16x \
  --signer-workflow engrmnaveed/diskspace-16x/.github/workflows/publish-cli.yml
ditto -x -k space16x-0.2.4-macos-universal.zip .
```

Move **Space 16X.app** to Applications in Finder.

### Allow the first launch

macOS blocks the first launch because the app isn't notarized by Apple. The project doesn't use a paid Apple Developer account. To check a download yourself, use the SHA-256 file and `gh attestation verify` as shown above. You only need to approve the app once.

1. Open Space 16X. macOS says it can't verify that the app is free of malware. Click **Done**. Don't click **Move to Trash**, which deletes the app.
2. Open **System Settings → Privacy & Security**.
3. Scroll to **Security**. Below **Allow applications from**, click **Open Anyway** next to the message about Space 16X.
4. Enter your password or use Touch ID, then click **Open Anyway** in the dialog that follows.

After that, the app opens normally.

**If Open Anyway isn't there:** the button appears only after you try to open the app, and for about an hour after that. Open Space 16X again, click **Done**, and return to Privacy & Security. If the app went to the Trash, reinstall it with `brew reinstall --cask space16x` or download the ZIP again.

To skip the prompt entirely, remove the quarantine attribute from a copy you have verified.

```sh
xattr -dr com.apple.quarantine "/Applications/Space 16X.app"
```

### Full Disk Access

macOS keeps some folders, such as other apps' containers, Mail, and Safari data, private to the apps that own them. A scan of your home folder or the startup disk lists them as protected items. Their contents aren't counted. To include them, give Space 16X Full Disk Access. The app only reads file metadata, and it never opens or changes file contents.

1. In Space 16X, click **Allow Full Disk Access…** in the protected-items notice, or open **System Settings → Privacy & Security → Full Disk Access**.
2. Turn on **Space 16X**. If it isn't listed, click **+** and choose it from Applications.
3. Return to Space 16X and click **Scan Again**. If items are still skipped, quit and reopen the app.

For the CLI, give Full Disk Access to the terminal app you run it from, such as Terminal or iTerm.

Some folders remain protected even with Full Disk Access. System Integrity Protection guards parts of the system, and some folders are readable only by root. Both apps list these items so the totals stay accurate about what they exclude.

Because the app is ad hoc signed rather than notarized, macOS may stop applying the setting after an update. If protected folders reappear after you upgrade, turn **Space 16X** off and on again in Full Disk Access.

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
