<p align="center">
  <img src="Branding/Lockup.png" width="540" alt="Space 16X by 16X Brains">
</p>

# Space 16X

Find the file or folder using your disk space. Space 16X is a read-only analyzer with a native macOS app and a scriptable CLI for macOS and Linux. Both use the same metadata scanner and work offline without an account.

## Analyze a folder on macOS

1. Choose a folder or local drive, or drop it into the Space 16X window.
2. Browse the ranked folder list or treemap, drill down with the breadcrumb, or switch to **Largest Files**.
3. Reveal a selected item in Finder or copy its path.

Allocated size is the default ranking. Switch to **Logical** to rank by file length. Both byte totals stay visible as you browse.

The treemap shows the largest items with a positive size. Remaining bytes are grouped as **Other Items**. Switch back to **List** to inspect each item.

## Use the CLI

```sh
space16x scan "$HOME/Downloads" --top 20
space16x scan "/Volumes/Work" --metric logical --top 5
space16x scan ./fixtures --top 5 --json > report.json
```

Exit code `0` means the scan completed, `2` means it returned partial results, and `1` means the scan could not start or complete. A partial JSON report carries its issues and totals on standard output. See the [CLI reference](docs/cli.md) and [example report](docs/example-report.json).

## Build and install

The app targets macOS 14 or later. The first Linux CLI release target is Ubuntu 24.04 x86-64. For a source checkout with Swift 6 installed:

```sh
swift build -c release --product space16x
"$(swift build -c release --show-bin-path)/space16x" --help
```

On macOS, build and open the app:

```sh
open "$(scripts/package-macos.sh)"
```

See the [installation guide](docs/install.md) for configuration, package verification, and keyboard shortcuts.

## How totals are counted

- Allocated bytes come from filesystem metadata. Logical bytes come from file length. Sparse files often have fewer allocated bytes than logical bytes.
- Directory totals sum regular-file bytes within the scan. A hard-linked file contributes once. Symbolic links and mounts on other devices are skipped.
- If a descendant cannot be inspected, the scan continues and marks its result as partial. Open the scan details to see which paths were affected.
- APFS clones can share blocks and snapshots can retain blocks. Scan totals can differ from volume usage and the amount of space reclaimed after deleting a file.
- Scan results reflect metadata observed during the traversal. Activity in the scanned folders can change results between runs.

## Contribute

Start with the [contributor guide](CONTRIBUTING.md) for the package layout, tests, and benchmark. See the [changelog](CHANGELOG.md) for release scope.

Built by [16X Brains](https://16xbrains.com/).
