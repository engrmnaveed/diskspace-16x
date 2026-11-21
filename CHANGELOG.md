# Changelog

## Unreleased

- See when files last changed in Space 16X Pro. The CLI explorer adds a **Modified** column with each item's newest modification date, a folder's being the newest of any file inside, and JSON reports add `newestModified` in Unix seconds. `space16x scan --older-than DAYS` counts only files last modified more than DAYS days ago, so sizes, rankings and top files show where old data sits. In the Mac app, with **Settings → Pro features** on, the folder list and Largest Files add a sortable **Modified** column. Pro reads the times with the same metadata calls as sizes, keeps them in a side table beside the scan tree, and Basic's memory and output are unchanged.
- See which file types use the most space in Space 16X Pro. `space16x scan --types N` lists the N largest types by extension with their size and file count, then the rest in one line, and `--json` writes them as `types`. Press `t` in the CLI explorer for the open folder's types. In the Mac app, with **Settings → Pro features** on, **Types** joins List, Treemap and Donut (or **Browse → File Types**, ⌘5). Both apps group files the same way: `Clip.MOV` and `clip.mov` are `.mov`, and `.bashrc` has no extension.
- See who fills a shared server in Space 16X Pro. `space16x scan --by-owner` lists the space and items each user owns, largest first, and `--json` writes them as `owners` with each `uid` and user name.

## 0.6.0

- Save a scan and browse it later or on another computer. `space16x scan --export FILE` saves the whole scan in ncdu's JSON format, and `--export -` writes it to standard output. `space16x --import FILE` or `--import -` opens it in the explorer, read-only, so `ssh -C server space16x scan / --export - | space16x --import -` scans a server and browses the result locally. ncdu and gdu open these exports, and Space 16X opens theirs. Without a terminal, `--import` prints the one-level report.
- Save and open scans in the Mac app. **File → Export Scan…** (⇧⌘E) saves the results in the same ncdu JSON format as the CLI, and **File → Open Scan…** (⇧⌘O) or dropping a `.json` file opens a scan saved by the app, the CLI, ncdu, or gdu. Opened scans are read-only: Rescan, Quick Look, Reveal in Finder, and Move to Trash are off until you scan a folder again.
- See which disk is filling up with `space16x volumes`: size, used and free space, use percentage, and inode use for each mounted volume, in text or with `--json`. It leaves out memory filesystems, snap images, bind mounts and macOS system volumes unless you pass `--all`. The Mac app's start screen lists your volumes with their free space, and clicking one scans it.
- Add Space 16X Pro, a second free edition built from the same source. The CLI's `pro` Cargo feature builds it, and each release publishes `space16x-pro-<target>.tar.gz` beside the Basic archive for all four targets. `install.sh --pro` installs Pro over Basic, and installing without it switches back. `space16x --version` ends with `(pro)` and JSON reports say `"edition": "pro"`. In Basic, a Pro command such as `space16x diff` prints a one-line hint to install Pro. The Mac app stays one app, with **Settings → Pro features**, off by default.
- Compare scans in Space 16X Pro. `space16x diff OLD NEW|PATH` compares a saved scan with another one, or with a folder scanned now using the saved scan's exclude patterns, and lists what grew or shrank one level down. `--depth N` goes deeper, `--top N` lists the files that changed most, including added and removed ones, and `--json` writes schema 3. In the Mac app, with **Settings → Pro features** on, **Compare…** above the folder list (or **Scan → Compare with Saved Scan…**, ⇧⌘K) adds a **Change** column, sortable by change, with the folder's total change and the items removed since then.
- Build the CLI with full link-time optimization and one codegen unit, which keeps the release archives smaller than 0.5.0's with export and import added. Scan speed is unchanged.
- Fix a trailing slash after the scan root in the total line of text reports, which 0.5.0 added.
- Refresh the promo Short to show a server's scan piped into the CLI explorer and the Mac app's Compare.

## 0.5.0

- Scan faster with less memory in the CLI. Scans read metadata on several threads, one per core up to 8, and `--threads N` sets the count for `scan` and the explorer. Use `--threads 1` on hard disks and network filesystems. Every thread count gives the same report. On macOS, the scanner reads each folder's file metadata in a single bulk call, so a 1,000,000-file scan takes about 1.2 seconds instead of 5.5. The scanner keeps each file's name instead of its full path, which cuts peak memory by more than half. The JSON schema and exit codes are unchanged. Issues are now listed in path order.
- Scan faster with less memory in the Mac app. The scanner reads several folders at once, up to 8, and a 1,000,000-file folder takes about 2.3 seconds instead of 5.6. Scanned items keep their names and share their folder's path instead of each storing a full path, which cuts peak memory by about two thirds. Results don't depend on the order folders finish in, including which path counts a hard-linked file.

## 0.4.0

- Move files and folders to the Trash from the CLI explorer. Press Space to mark items, press `d`, and confirm the count and size with `y`. Folder totals and rankings update in place without a rescan. Space is freed when you empty the Trash. On Linux, items go to the FreeDesktop.org Trash that file managers use, and they are never copied to another disk. `--read-only` or `SPACE16X_READ_ONLY=1` turns trashing off. Cloud-only, protected, and incompletely scanned items stay where they are.
- Move files and folders to the Trash from the Mac app. Select one or more rows with ⌘-click or ⇧-click, then press ⌘⌫, choose **Browse → Move to Trash…**, use the row's context menu, or click **Move to Trash** in the selection bar. An alert confirms the item count and allocated size. The list, charts, and totals update in place without a rescan. Cloud-only, protected, unscanned, and incompletely read items stay where they are, as in the CLI explorer.
- Rescan one folder without rescanning everything. Press `r` in the CLI explorer, or choose **Scan → Rescan Folder** (⌥⌘R) or **Rescan Folder** in a folder's context menu in the Mac app. The new results replace that folder in place, and the totals and item counts above it update. Exclude patterns keep applying relative to the scanned folder. Cancelling keeps the earlier results.
- Preview the selected file or folder with Quick Look in the Mac app. Press Space, or choose **Browse → Quick Look** (⌘Y) or **Quick Look** in the context menu. The preview follows the selection, as in Finder, and Space closes it. Cloud-only items aren't previewed, because that would download them.
- Refresh the promo Short to show marking items in the CLI explorer and the Move to Trash prompt.

## 0.3.0

- Add apparent sizes and item counts to the CLI. `space16x scan --apparent` shows and ranks file lengths, as `ls -l` reports them, instead of allocated bytes, which helps with sparse and compressed files. JSON reports move to schema 3, which keeps every schema 2 field and adds `apparentBytes`, `itemCount` (files and folders below an entry, counting each hard-linked file once), `rankedBy` and `edition`. Exit codes are unchanged.
- Skip files and folders by pattern in the CLI explorer and reports with `--exclude PATTERN` and `--exclude-from FILE`. Patterns follow a small `.gitignore` subset: `node_modules` or `*.log` match a name anywhere, `/build` anchors to the scan root, `**` spans folders, and `cache/` matches folders only. Excluded entries aren't read, keep the scan complete, and appear as `excludedCount` and `excludes` in JSON.
- Sort the CLI explorer's folder contents by size, item count, or name with `s`, and switch between allocated and apparent sizes with `a`. Wide terminals show an Items column, and file details show both sizes. The explorer honors `NO_COLOR` and a new `--no-color` option.
- Sort the Mac app's folder list and Largest Files by name, size, or item count from the column headers, with a new Items column. **View → Size** switches the tables, charts, and totals between allocated and apparent sizes and remembers the choice. **Settings → Exclude from scans** takes the same patterns as the CLI, one per line, and the header and Scan Details report how many items were excluded.
- Refresh the promo Short and the README preview to show the Mac app's sortable folder list with item counts.

## 0.2.4

- Treat folders the system won't let apps read as protected items rather than scan errors. A scan of `/` used to show an orange "Partial scan" with hundreds of skipped paths, though nearly all were expected macOS restrictions. Both apps now keep the orange warning for real errors and show a neutral notice for protected items. The Mac app groups them by folder in Scan Details and offers Full Disk Access when it would unlock some of them. The CLI summarizes them in one stderr line and marks them `[protected]` and Protected in the explorer. JSON output and exit codes are unchanged.
- Show CLI sizes in decimal units (KB, MB, GB), the way Finder, System Settings, and the Mac app do, instead of binary KiB, MiB, and GiB. The same folder now shows the same number everywhere, for example 245.33 GB rather than 228.5 GiB. The CLI explorer's file details and the Mac app's size tooltips also show the binary size next to the exact byte count. JSON reports are unchanged and still give exact bytes.

## 0.2.3

- Stop double-counting the macOS startup disk. A scan of `/` reached each APFS firmlinked folder, such as `/Users`, both at its visible path and again under `/System/Volumes/Data`, which roughly doubled the total. Both scanners now count those folders once at their visible paths.
- Spell out the first launch of the Mac app in the README, the installation guide, and the Homebrew caveats: click Done rather than Move to Trash, find Open Anyway under Security, and confirm with a password or Touch ID.

## 0.2.2

- Complete scans of very deep folder trees. Both scanners raise the open-file soft limit, which macOS sets to 256 for apps launched by launchd, so deep trees are no longer reported as partial with "Too many open files".
- Escape Unicode bidirectional control characters in terminal paths and messages, so a crafted filename can't display as different text.
- Restore the terminal before printing a panic message from the explorer.
- Publish `install.sh` with each CLI release and attest build provenance for the archives and installer. Pin the one-liners to a reviewed installer commit while the current release is available. Use the `release` environment to apply publication rules.
- Add a security policy, Dependabot updates, and a cargo-deny check for advisories, licenses, and dependency sources.
- Publish the universal Mac app with each release as an attested, ad hoc signed ZIP, along with a Homebrew cask. Install it with `brew install --cask engrmnaveed/tap/space16x`. The app isn't notarized, so the installation guide explains the one-time approval on first launch.
- Build the universal Mac app archive and check the rendered Homebrew cask with `brew style` on every pull request, so packaging problems surface before a release tag.

## 0.2.1

- Install or remove the CLI with a single curl or GNU Wget command, including setup and cleanup for PATH.
- Publish checksummed CLI binaries for macOS and Linux from version tags.

## 0.2.0 · Local release candidate

- Rust CLI with an interactive Ratatui explorer on macOS and Linux, including allocated-size browsing, filter, largest files, scan details, and cancellation.
- One-level terminal reports by default, with depth and largest-file options. JSON schema 2 reports allocated bytes and preserves undecodable path bytes where present.
- Candidate CLI archives for Apple silicon, Intel Mac, Linux x86-64, and Linux ARM64. The GitHub release installer verifies SHA-256 and supports per-user installation.

## 0.1.1 · Local update

- Avoid macOS cloud downloads during scanning. Show cloud-only items as references with zero counted bytes, and continue past stalled provider metadata with a partial-scan report.
- Show a two-level folder preview during larger scans. Update completed folders in the background while the results remain browsable.
- Use allocated size throughout the Mac app and add a donut chart for the selected folder's usage.

## 0.1.0 · Local release candidate

- Native macOS folder and local-drive scanning with Finder selection and drag-and-drop.
- Directory navigation, a folder treemap, allocated and logical size rankings, largest files, Finder reveal, and copy path.
- Scan progress, cancellation, and details for skipped paths.
- Shared Swift scanner with metadata-only traversal, hard-link accounting, and cross-device exclusions.
- macOS and Linux CLI with a versioned JSON format and distinct complete, partial, and fatal exit codes.
- Reproducible test fixtures and benchmark, plus macOS and Ubuntu release packaging.
