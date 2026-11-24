# Install DiskUse16x

## Install the CLI from a release

In Bash, Zsh, or a POSIX shell, paste either one-liner.

```sh
diskuse16x_path=$(curl --proto '=https' --tlsv1.2 -fsSL https://github.com/engrmnaveed/diskuse16x/releases/latest/download/install.sh | sh -s -- --shell-path) && [ -n "$diskuse16x_path" ] && export PATH="$diskuse16x_path" && unset diskuse16x_path
```

```sh
diskuse16x_path=$(wget -qO- https://github.com/engrmnaveed/diskuse16x/releases/latest/download/install.sh | sh -s -- --shell-path) && [ -n "$diskuse16x_path" ] && export PATH="$diskuse16x_path" && unset diskuse16x_path
```

The one-liners run the `install.sh` that the latest release publishes, so the installer always matches that release's archives. Each release attests it with GitHub build provenance. To review it first, download it from the URL above, read it, check it with `gh attestation verify install.sh` as [Verify a release archive](#verify-a-release-archive) shows, and run `sh install.sh`.

Run `diskuse16x --version` or open the explorer with `diskuse16x`, or with its short name `du16x`, which the installer puts beside it. The installer checks the archive's SHA-256 digest and installs to `$HOME/.local/share/diskuse16x/bin`. It sets up your shell's startup file when needed, and the one-liner updates the current shell's PATH. Use curl or GNU Wget for the asset download. On a BusyBox-only image, install curl or use the manual archive instructions.

Prefer a package manager? Use [Homebrew](#install-the-cli-with-homebrew) on macOS or Linux, or a [.deb or .rpm package](#install-a-linux-package) on Linux.

Pass `--version 1.0.0` after `--shell-path` to select a published version from 1.0.0 on. Releases before 1.0 were named Space 16X: install one of them with the `install.sh` attached to that release. Use `--bin-dir PATH` to choose another directory or `--repo OWNER/REPO` for a public mirror with the same asset names. [GitHub Releases](https://github.com/engrmnaveed/diskuse16x/releases)

## Uninstall the managed CLI

Paste either line to remove the executable, installer files, and shell startup entries created by the installer. The command updates PATH in the current shell. It preserves pre-existing PATH entries and manually installed executables.

```sh
diskuse16x_path=$(curl --proto '=https' --tlsv1.2 -fsSL https://github.com/engrmnaveed/diskuse16x/releases/latest/download/install.sh | sh -s -- --uninstall --shell-path) && [ -n "$diskuse16x_path" ] && export PATH="$diskuse16x_path" && unset diskuse16x_path
```

```sh
diskuse16x_path=$(wget -qO- https://github.com/engrmnaveed/diskuse16x/releases/latest/download/install.sh | sh -s -- --uninstall --shell-path) && [ -n "$diskuse16x_path" ] && export PATH="$diskuse16x_path" && unset diskuse16x_path
```

## Install DiskUse16x Pro

DiskUse16x comes in two free editions. Basic, which the commands above install, is the lightweight default. Pro adds more analysis, starting with `diskuse16x diff`, which compares two scans. Both install the same `diskuse16x` command in the same place, so installing one edition replaces the other. Add `--pro` to the installer:

```sh
diskuse16x_path=$(curl --proto '=https' --tlsv1.2 -fsSL https://github.com/engrmnaveed/diskuse16x/releases/latest/download/install.sh | sh -s -- --pro --shell-path) && [ -n "$diskuse16x_path" ] && export PATH="$diskuse16x_path" && unset diskuse16x_path
```

```sh
diskuse16x_path=$(wget -qO- https://github.com/engrmnaveed/diskuse16x/releases/latest/download/install.sh | sh -s -- --pro --shell-path) && [ -n "$diskuse16x_path" ] && export PATH="$diskuse16x_path" && unset diskuse16x_path
```

`diskuse16x --version` ends with `(pro)` in the Pro edition. Run the Basic one-liner again to switch back. `--uninstall` removes either edition. In Basic, a Pro command such as `diskuse16x diff` prints a one-line hint to install Pro and exits with code 1. Releases from 0.6.0 publish `diskuse16x-pro-<target>.tar.gz` beside each Basic archive, built from the same tag. The [Pro CLI reference](cli-pro.md) documents Pro's commands. In the Mac app, Pro features are part of the one app: turn them on in **Settings → Pro features**.

## Fish shell

Use Fish's list syntax to apply PATH immediately. Add `--uninstall` before `--shell-path` to remove the installation. The download can also use `wget -qO-`.

```fish
set -l diskuse16x_path (curl --proto '=https' --tlsv1.2 -fsSL https://github.com/engrmnaveed/diskuse16x/releases/latest/download/install.sh | sh -s -- --shell-path); and test -n "$diskuse16x_path"; and set -gx PATH (string split : -- "$diskuse16x_path")
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

Download `diskuse16x-<target>.tar.gz`, or `diskuse16x-pro-<target>.tar.gz` for Pro, and its companion `.sha256` file from the same release. The commands below use the Apple silicon archive.

```sh
archive=diskuse16x-aarch64-apple-darwin.tar.gz
shasum -a 256 -c "$archive.sha256"
tar -xzf "$archive" diskuse16x
mkdir -p "$HOME/.local/bin"
install -m 0755 diskuse16x "$HOME/.local/bin/diskuse16x"
```

On Linux, use `sha256sum -c "$archive.sha256"` and substitute your target. The [CLI reference](cli.md) has terminal key bindings and report examples, and Pro archives add the [Pro CLI reference](cli-pro.md).

### Verify a release archive

The SHA-256 file only proves the download wasn't corrupted, because it comes from the same place as the archive. Each release, starting with 0.2.2, also publishes a GitHub build-provenance attestation for every archive and for `install.sh`, and from 1.0.0 for every `.deb` and `.rpm` package. The attestation shows that the file was built by this repository's release workflow from the tagged commit. Check it with the [GitHub CLI](https://cli.github.com/):

```sh
gh attestation verify diskuse16x-aarch64-apple-darwin.tar.gz \
  --repo engrmnaveed/diskuse16x \
  --signer-workflow engrmnaveed/diskuse16x/.github/workflows/publish-cli.yml
```

Releases before 1.0 were attested under the repository's earlier name, so verify their files with `engrmnaveed/diskspace-16x` in both `--repo` and `--signer-workflow`. Use the same command with `install.sh` to verify the installer, or with a package name, such as `diskuse16x_amd64.deb`, to verify a Linux package. Add `--source-ref` with the full tag ref, such as `refs/tags/vX.Y.Z`, to require a specific release tag.

## Install the CLI with Homebrew

Releases from 1.0.0 publish Homebrew formulas for the CLI on macOS and Linux, in the same tap as the Mac app. `du16x` installs Basic, and `du16x-pro` installs Pro. Homebrew 6 and later load formulas only from taps you trust, so run `brew trust` once.

```sh
brew tap engrmnaveed/tap
```

```sh
brew trust engrmnaveed/tap
```

```sh
brew install engrmnaveed/tap/du16x
```

For Pro, install `engrmnaveed/tap/du16x-pro` instead. Both formulas install the `diskuse16x` command, so Homebrew keeps one at a time: run `brew uninstall du16x` before installing Pro, and the reverse to switch back. Update with `brew upgrade du16x`, and remove with `brew uninstall du16x`. Use `du16x-pro` in both commands for Pro.

Homebrew installs into its own folder, apart from the installer's `$HOME/.local/share/diskuse16x/bin`. If you install both ways, the one earlier on your PATH runs, so remove the other.

## Install a Linux package

Releases from 1.0.0 attach `.deb` and `.rpm` packages, built from the same static executables as the Linux archives. A package installs `/usr/bin/diskuse16x` for every user on the computer, so it needs `sudo`.

| Edition and CPU | Debian, Ubuntu | Fedora, RHEL, openSUSE |
| --- | --- | --- |
| Basic, x86-64 | `diskuse16x_amd64.deb` | `diskuse16x.x86_64.rpm` |
| Basic, ARM64 | `diskuse16x_arm64.deb` | `diskuse16x.aarch64.rpm` |
| Pro, x86-64 | `diskuse16x-pro_amd64.deb` | `diskuse16x-pro.x86_64.rpm` |
| Pro, ARM64 | `diskuse16x-pro_arm64.deb` | `diskuse16x-pro.aarch64.rpm` |

The names stay the same in every release, so `releases/latest/download/` always points at the newest one. On Debian or Ubuntu, download the package for your CPU and install it with apt. The commands below use Basic for x86-64.

```sh
curl -fLO https://github.com/engrmnaveed/diskuse16x/releases/latest/download/diskuse16x_amd64.deb
```

```sh
sudo apt install ./diskuse16x_amd64.deb
```

Installing the Pro package replaces Basic, and the reverse. Remove the command with `sudo apt remove diskuse16x`, or `diskuse16x-pro` for Pro.

On Fedora or RHEL, use dnf:

```sh
curl -fLO https://github.com/engrmnaveed/diskuse16x/releases/latest/download/diskuse16x.x86_64.rpm
```

```sh
sudo dnf install ./diskuse16x.x86_64.rpm
```

The packages aren't signed, so dnf notes that it skipped OpenPGP checks. Check the download with its `.sha256` file or `gh attestation verify`, as [Verify a release archive](#verify-a-release-archive) shows. To switch editions, add `--allowerasing`, as in `sudo dnf install --allowerasing ./diskuse16x-pro.x86_64.rpm`. Remove the command with `sudo dnf remove diskuse16x`.

On openSUSE, install with `sudo zypper install --allow-unsigned-rpm ./diskuse16x.x86_64.rpm`. To switch editions, remove the other one first with `sudo zypper remove diskuse16x`.

To update a package, download the new release's package and install it the same way.

## Build the CLI from source

Install [Rustup](https://rustup.rs/) and a native linker for your system. Run the build from a checkout of the repository.

```sh
(cd cli && cargo install --locked --path . --root "$HOME/.local")
"$HOME/.local/bin/diskuse16x" --help
```

Add `--features pro` to the `cargo install` command to build the Pro edition. The toolchain file under `cli/` selects the project compiler. To make a distributable package, use [the packaging instructions](release.md#package-the-cli-locally).

## Build the Mac app from source

The Mac app targets macOS 14. Build from the checkout on a Mac with Swift 6 or later and the Xcode Command Line Tools.

```sh
scripts/package-macos.sh
open "dist/DiskUse16x.app"
```

The app in `dist` is built for the current Mac and ad hoc signed for local development. Use `scripts/package-macos.sh --universal` to build both arm64 and x86-64 slices.

## Install the Mac app

Releases from 0.2.2 publish a universal Mac app for Apple silicon and Intel on macOS 14 or later. The app is ad hoc signed and is not notarized by Apple, because the project doesn't use a paid Apple Developer account. The one-command installer is the easiest way in. With Homebrew or the ZIP, macOS blocks the first launch until you allow it once.

### Install with one command

From 1.0.0, paste this into Terminal:

```sh
curl --proto '=https' --tlsv1.2 -fsSL https://github.com/engrmnaveed/diskuse16x/releases/latest/download/install.sh | sh -s -- --app
```

It downloads the newest release's app ZIP, checks its SHA-256 checksum and the app's signature, and installs **DiskUse16x.app** into the Applications folder in your home folder, `~/Applications`, without an admin password. Spotlight and Launchpad find it there. If an earlier copy is in that folder, it replaces it. Quit DiskUse16x first.

The app opens on its first launch without the **Open Anyway** step. macOS checks apps that carry the quarantine flag that browsers add to downloads, and curl doesn't add it. Tested on macOS 27.2: macOS's own assessment rejects the ad hoc app, and the installed copy still opens. This skips that one-time check on purpose, as the `xattr` command under [Allow the first launch](#allow-the-first-launch) does. The installer checks the checksum and signature instead, and `gh attestation verify` can confirm that the ZIP came from this project's release workflow, as [Install the release ZIP](#install-the-release-zip) shows.

Run the same command to update the app. Add `--version 1.0.0` after `--app` for a particular release. Remove the app with `--uninstall --app` in place of `--app`. Your settings and saved scans stay.

### Install with Homebrew

The cask lives in the project's tap. Add the tap, then install the app. Homebrew 6 and later only load casks from taps you trust, so run `brew trust` once before installing.

```sh
brew tap engrmnaveed/tap
```

```sh
brew trust engrmnaveed/tap
```

```sh
brew install --cask engrmnaveed/tap/diskuse16x
```

Upgrade with `brew upgrade --cask diskuse16x`. Remove the app with `brew uninstall --cask diskuse16x`. Add `--zap` to that command to also remove its preferences and saved window state.

### Install the release ZIP

Download `diskuse16x-<version>-macos-universal.zip` and its `.sha256` file from the [releases page](https://github.com/engrmnaveed/diskuse16x/releases). Verify the archive and expand it from the download directory. The commands below use version 0.9.1 as an example.

```sh
shasum -a 256 -c diskuse16x-0.9.1-macos-universal.zip.sha256
gh attestation verify diskuse16x-0.9.1-macos-universal.zip \
  --repo engrmnaveed/diskuse16x \
  --signer-workflow engrmnaveed/diskuse16x/.github/workflows/publish-cli.yml
ditto -x -k diskuse16x-0.9.1-macos-universal.zip .
```

Move **DiskUse16x.app** to Applications in Finder.

### Allow the first launch

With Homebrew or the release ZIP, macOS blocks the first launch because the app isn't notarized by Apple. The [one-command installer](#install-with-one-command) avoids this step. The project doesn't use a paid Apple Developer account. To check a download yourself, use the SHA-256 file and `gh attestation verify` as shown above. You only need to approve the app once.

1. Open DiskUse16x. macOS says it can't verify that the app is free of malware. Click **Done**. Don't click **Move to Trash**, which deletes the app.
2. Open **System Settings → Privacy & Security**.
3. Scroll to **Security**. Below **Allow applications from**, click **Open Anyway** next to the message about DiskUse16x.
4. Enter your password or use Touch ID, then click **Open Anyway** in the dialog that follows.

After that, the app opens normally.

**If Open Anyway isn't there:** the button appears only after you try to open the app, and for about an hour after that. Open DiskUse16x again, click **Done**, and return to Privacy & Security. If the app went to the Trash, reinstall it with `brew reinstall --cask diskuse16x` or download the ZIP again.

To skip the prompt entirely, remove the quarantine attribute from a copy you have verified.

```sh
xattr -dr com.apple.quarantine "/Applications/DiskUse16x.app"
```

### Full Disk Access

macOS keeps some folders, such as other apps' containers, Mail, and Safari data, private to the apps that own them. A scan of your home folder or the startup disk lists them as protected items. Their contents aren't counted. To include them, give DiskUse16x Full Disk Access. The app only reads file metadata, and it never opens or changes file contents.

1. In DiskUse16x, click **Allow Full Disk Access…** in the protected-items notice, or open **System Settings → Privacy & Security → Full Disk Access**.
2. Turn on **DiskUse16x**. If it isn't listed, click **+** and choose it from Applications.
3. Return to DiskUse16x and click **Scan Again**. If items are still skipped, quit and reopen the app.

For the CLI, give Full Disk Access to the terminal app you run it from, such as Terminal or iTerm.

Some folders remain protected even with Full Disk Access. System Integrity Protection guards parts of the system, and some folders are readable only by root. Both apps list these items so the totals stay accurate about what they exclude.

Because the app is ad hoc signed rather than notarized, macOS may stop applying the setting after an update. If protected folders reappear after you upgrade, turn **DiskUse16x** off and on again in Full Disk Access.

## Update DiskUse16x

DiskUse16x never checks for updates on its own, and the app never connects to the network. To see whether a newer version is out, choose **Help → Check for Updates…** in the Mac app, which opens the newest release's page in your browser. Compare its version with **DiskUse16x → About DiskUse16x**, or with `diskuse16x --version` in the terminal.

- **CLI from the installer:** run the [one-liner](#install-the-cli-from-a-release) again, with `--pro` for Pro. It replaces the command with the newest release.
- **CLI from Homebrew:** run `brew upgrade du16x`, or `brew upgrade du16x-pro` for Pro.
- **Linux package:** download the new release's package and install it the same way, as [Install a Linux package](#install-a-linux-package) shows.
- **Mac app from the one-command installer:** run [the same command](#install-with-one-command) again.
- **Mac app from Homebrew:** run `brew upgrade --cask diskuse16x`.
- **Mac app from the ZIP:** quit DiskUse16x, then download, verify and expand the new ZIP as in [Install the release ZIP](#install-the-release-zip), and drag the new **DiskUse16x.app** into Applications to replace the old one.

With Homebrew or the ZIP, the new app is a new download that isn't notarized, so macOS may ask you to [allow the first launch](#allow-the-first-launch) again. If protected folders come back after an update, turn **DiskUse16x** off and on again in [Full Disk Access](#full-disk-access).

## Upgrading from Space 16X

DiskUse16x was called Space 16X before 1.0. Upgrading the usual way moves you to the new name and keeps what you saved.

- **CLI from the installer:** run the new [one-liner](#install-the-cli-from-a-release). It installs `diskuse16x` and `du16x`, then removes the `space16x` command that the earlier installer added, with its shell setup. If you chose a folder with `--bin-dir`, the new commands go there. An executable you replaced yourself stays.
- **Homebrew:** `brew upgrade` moves the `space16x` cask to `diskuse16x`, which replaces **Space 16X.app** with **DiskUse16x.app**.
- **The release ZIP:** install **DiskUse16x.app** as usual, then move **Space 16X.app** from Applications to the Trash.
- **Your settings and saved scans:** on its first launch, the app copies Space 16X's settings, such as **Pro features**, excludes and the menu-bar monitor. The app and the CLI keep using a scan history that Space 16X saved, and both open its exports. `SPACE16X_READ_ONLY` still works when `DISKUSE16X_READ_ONLY` isn't set.
- **Full Disk Access:** macOS sees a new app, so turn on **DiskUse16x** in [Full Disk Access](#full-disk-access) and turn off **Space 16X**, or remove it from the list.

Scripts that call `space16x` need the new name, `diskuse16x` or `du16x`. Everything else they rely on stays the same, and the [1.x promise](cli.md#stability-in-1x) starts with the new name.

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
| Pro: open a new window with its own scan | ⌘N |
| Pro: show file types, cleanup suggestions or duplicates | ⌘5, ⌘6, ⌘7 |
| Pro: compare with a saved scan | ⇧⌘K |
| Pro: save the scan to the history | ⌥⌘S |
| Pro: explain the disk's space | ⇧⌘X |

If access to a folder is restricted, open **Scan Details** to see any skipped paths, or scan a location you can read. The app and CLI keep scan data in memory. Redirect CLI output to save a report intentionally.
