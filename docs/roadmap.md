# Space 16X roadmap: Basic and Pro editions (both free)

This file is the durable record of the roadmap and its progress. Work on it can stop at any time, for example when a usage limit is reached, so any session must be able to pick it up from here and from git alone.

## Progress

Status key: `[x]` landed on `main` · `[~]` in progress (see **Resume here**) · `[ ]` not started.

**Resume here** — Roadmap complete: paused for owner review after v0.9.0. Milestones 1–7 shipped as v0.3.0–v0.9.0. M7 (Complete and stable) fixed the owner's v0.8.0 QA findings (clone-aware duplicates, file mounts in `volumes`, Pro discoverability, release notes from the changelog) and finished what M6 left for later: an Exclusive column, Duplicates and Explain in the explorer, snapshot dates, footprint budgets, review suggestions, and the Mac's older-than filter and HTML report. Nothing comes after M7. Cross-platform work waits for the owner's feasibility review (see **Future**). Basic's published Linux x86-64 archive is 590,199 bytes against its 600 KB budget. The items still for the owner's eyes are under M7.9.

### M1 (v0.3.0) Focus [Basic]
- [x] M1.1 Rust: `apparentBytes` and `itemCount` in the scanner, JSON schema 3 (`apparentBytes`, `itemCount`, `rankedBy`, `edition`), `scan --apparent`
- [x] M1.2 Rust: `--exclude` / `--exclude-from` (scan and TUI), `excludedCount` / `excludes` in JSON
- [x] M1.3 TUI: `s` sort (size → items → name), Items column, `a` apparent toggle, `NO_COLOR` / `--no-color`
- [x] M1.4 Swift: apparent size, item counts, exclude rules in `SpaceCore`, shared parity fixture
- [x] M1.5 Mac: Items column, sortable columns, View → Size menu, Settings scene with exclude patterns, plus the promo refresh covering the TUI and Mac changes (`AGENTS.md` wants it in the same change)
- [x] M1.6 Docs (`docs/cli.md`, README, CHANGELOG, manual QA, PRD/AGENTS edition model), benchmark in `docs/qa.md`, CI parity checks
- [x] M1.7 Release v0.3.0

### M2 (v0.4.0) Act [Basic]
- [x] M2.1 Rust: Trash backends, macOS `trashItemAtURL` through a minimal Objective-C binding (no new crate) and an in-house Linux FreeDesktop Trash (home Trash, else `$topdir/.Trash/$uid` or `$topdir/.Trash-$uid`, `.trashinfo` written first, never across devices), plus the write rule in `AGENTS.md`, `SECURITY.md`, `PRD.md` and `README.md`
- [x] M2.2 TUI: Space marks items, `d` moves them to the Trash after a `y/N` prompt, the tree updates in place, "Space is freed when you empty the Trash", cloud-only and protected items blocked, `--read-only` and `SPACE16X_READ_ONLY=1`, a pty check in `scripts/check-tui.py`
- [x] M2.3 Mac: multi-select, ⌘⌫ Move to Trash, a confirmation sheet with the item count and size, `FileManager.trashItem`, cloud-only and protected items blocked, the tree updated in place
- [x] M2.4 Rescan folder: TUI `r` and Mac **Rescan Folder**, replacing one subtree in place
- [x] M2.5 Mac: Quick Look on Space
- [x] M2.6 Docs (`docs/cli.md`, README, CHANGELOG, manual QA rows for Trash; check on a Mac whether Finder offers Put Back for items moved with `trashItem` and document the answer) and the promo refresh showing Move to Trash
- [x] M2.7 Release v0.4.0

### M3 (v0.5.0) Speed [Basic]
- [x] M3.1 Rust: lean arena tree, with name-only nodes in depth-first order, parent indices and subtree sizes, and paths rebuilt on demand. The walk stores one record per directory and a compose pass builds the tree, assigning each hard-linked file to its first path in depth-first order. Reports, the TUI, Move to Trash and rescan use the arena.
- [x] M3.2 Rust: parallel walk with `--threads N` (1 to 64, default `min(cores, 8)`, `1` for HDDs and NFS), std threads and a shared work queue, documented in `docs/cli.md`, README and CHANGELOG. Output stays deterministic because hard links are assigned in the compose pass, so workers share no hard-link set. Tests cover repeated runs, unreadable folders and hard links across threads.
- [x] M3.3 Rust on macOS: read each directory's names and regular files' metadata with `getattrlistbulk`, keeping `fstatat` for folders, entries with errors and filesystems without bulk support. At 1M files, per-file `fstatat` thrashes the vnode cache and threads contend in the kernel.
- [x] M3.4 Swift: entries keep their name and a shared parent location instead of a full path
- [x] M3.5 Swift: TaskGroup walk with the same deterministic hard-link rule
- [x] M3.6 Benchmarks at 100K and 1M files in `docs/qa.md`
- [x] M3.7 Release v0.5.0

### M4 (v0.6.0) Carry, compare, editions [Basic + Pro scaffolding]
- [x] M4.1 Rust: `scan --export FILE|-` writes ncdu JSON v1 with `space16x_*` keys (states, issues, excludes, non-UTF-8 names), and `--import FILE|-` opens a saved scan in the explorer, read-only, including ncdu and gdu exports and a pipe from `ssh`. A streaming reader and writer in `cli/src/ncdu.rs` with no new dependency. Full LTO and one codegen unit keep the archive under budget.
- [x] M4.2 Mac: **Open Scan…** and **Export Scan…** in the same format (`SpaceCore/SavedScan.swift`), opened scans read-only, and shared ncdu, gdu and Space 16X fixtures in `fixtures/scan-exports/` that both importers' tests read
- [x] M4.3 Basic volumes: `space16x volumes [--all] [--json]` (size, used, free and inodes from `statvfs`, or `statfs` on macOS) in `cli/src/volumes.rs`, and a volume list on the Mac start screen that scans the volume you click
- [x] M4.4 Edition scaffolding: the `pro` Cargo feature, Basic hints for Pro subcommands, `(pro)` in `--version` and `"edition": "pro"`, `space16x-pro-<target>` archives from `package-cli.sh --pro`, `install.sh --pro` (one-liners re-pinned), both editions linted, tested and checked in the existing CI jobs (job names unchanged), Mac **Settings → Pro features** and a `SpacePro` target, docs
- [x] M4.5 Pro: `space16x diff OLD NEW|PATH [--depth N | --top N] [--apparent] [--json]` in `cli/src/pro/diff.rs` (a folder NEW is scanned with OLD's excludes), and Mac **Compare** (`SpacePro/ScanComparison.swift`): a sortable Change column, the folder's change and removed items, shown when **Settings → Pro features** is on
- [x] M4.6 Promo refresh (an import piped from `ssh` and Pro's Compare, 29 s), manual QA rows, benchmarks and both editions' archive sizes in `docs/qa.md`
- [x] M4.7 Release v0.6.0

### M5 (v0.7.0) Pro analysis [Pro]
- [x] M5.1 Newest modification time: Rust side tables keyed by `NodeId` with each entry's modification time and owner, read by the same `fstatat` and `getattrlistbulk` calls (empty in Basic), a TUI **Modified** column, `newestModified` in Pro JSON, and `scan --older-than DAYS` in `cli/src/pro/`; a Mac **Modified** column with the same rule, and a shared fixture
- [x] M5.2 File types: `scan --types N` and the TUI `t` view (the largest types by extension in the open folder) in `cli/src/pro/types.rs`, and a Mac **Types** view in `SpacePro` with the same grouping
- [x] M5.3 Owners: `scan --by-owner` (bytes and items per user, names from the user database) in `cli/src/pro/owners.rs`. CLI only: the Mac app scans one user's files.
- [x] M5.4 Scan history: `space16x history` (save a scan, list saved scans, compare a folder with its saved scans through `diff`) under `$XDG_STATE_HOME/space16x/history`, and Mac saved scans (**Save to History**, Compare with a saved scan) under `~/Library/Application Support/Space 16X/History`. Only an explicit save writes anything.
- [x] M5.5 Promo refresh, manual QA rows, benchmarks and both editions' archive sizes in `docs/qa.md`
- [x] M5.6 Release v0.7.0

### M6 (v0.8.0) Pro deep [Pro]
- [x] M6.1 Explain my disk: `space16x explain [PATH] [--json]` in `cli/src/pro/explain.rs` shows the space a scan can't see on PATH's volume. On macOS: the APFS container's volumes with each one's used space (`ATTR_VOL_SPACEUSED`), purgeable space (important-usage capacity minus free space) and Time Machine local snapshots (`tmutil listlocalsnapshots`). On Linux: deleted files that processes still hold open (`/proc/*/fd`). System queries and metadata only. Mac: **Scan → Explain Disk Space** and a volume's context menu, from `SpacePro/DiskExplanation.swift`. The Pro commands' reference moved to `docs/cli-pro.md`, which only the Pro archive ships, so Pro docs no longer count against Basic's budget.
- [x] M6.2 Smart cleanup suggestions: one rule table (Xcode DerivedData and device support, simulator caches, `node_modules`, Cargo `target` beside `Cargo.toml`, the Gradle and npm caches, `Library/Caches`, `.cache`) matched by name against the finished scan, with no extra reads, in `cli/src/pro/cleanup.rs` and `SpacePro/CleanupSuggestions.swift`. `scan --cleanup` lists them, and the explorer's `u` view (not `x`, which cancels a scan) lists them for Space and `d`, Basic's confirmed Move to Trash. Mac: **Cleanup** in the sidebar feeds the same confirmation. `fixtures/cleanup` drives both apps' tests. Xcode archives aren't suggested, because they can't be recreated.
- [x] M6.3 Clone-aware exclusive size: `scan --exclusive` measures, after the scan and on its threads, each counted file's shared bytes (`ATTR_CMNEXT_PRIVATESIZE` on macOS, the link count and, on Btrfs, XFS and bcachefs, `FIEMAP` shared extents on Linux) in `cli/src/pro/exclusive.rs`, keeps only sharing files and their folders' totals in a sparse table keyed by `NodeId`, and reports `exclusiveBytes`, the space that moving an item to the Trash and emptying it frees at least. The scanner itself is unchanged. Mac: the Move to Trash confirmation shows the same figure for the selection, measured for up to two seconds (`SpacePro/ExclusiveSize.swift`).
- [x] M6.4 Opt-in duplicates: `space16x duplicates [PATH] [--min-size SIZE] [--json] [--threads N] [EXCLUDES]` in `cli/src/pro/duplicates.rs` scans metadata, groups files by length, and reads only same-length candidates on the scan's threads, first 64 KB and then whole files, hashed with std's SipHash under random keys. Groups are ranked by the space the extra copies use, and unreadable files are issues (exit 2). Mac: **Duplicates** in the sidebar compares the current scan's files only after **Find Duplicates** is clicked (`SpacePro/DuplicateFinder.swift`, Swift's seeded `Hasher`), and lists the copies for Move to Trash. These are the only readers of file contents, which `AGENTS.md` and `SECURITY.md` now say.
- [x] M6.5 HTML report and terminal treemap: `scan --html FILE|-` writes one self-contained page (totals, a treemap of the 30 largest items, the root's contents, the 25 largest files, file types and, on shared servers, owners, with no scripts or network, light and dark) in `cli/src/pro/html.rs`, and the explorer's `m` draws the open folder as a treemap with the selection highlighted, both from one squarified layout in `cli/src/pro/treemap.rs`. CLI only: the Mac app already has its treemap.
- [x] M6.6 Mac menu-bar free-space monitor: **Settings → Pro features → Show free space in the menu bar** (off by default) inserts a `MenuBarExtra` with the startup disk's free space and every volume's in its menu (`SpaceApp/FreeSpaceMonitor.swift`). It follows the setting through `UserDefaults`, refreshes once a minute only while shown, and posts one notification when free space drops below the chosen threshold (10 GB by default, or never), rearming after it rises 10% above it (`SpacePro/LowSpaceAlert.swift`).
- [x] M6.7 Mac Build Footprint receipts: while comparing, **Scan → Save Footprint Receipt…** or the receipt button beside the comparison asks for the operation's name and writes a Markdown receipt the user names (`SpacePro/FootprintReceipt.swift`): both scan times, the totals and item counts, what files gained and lost, and the 25 largest file changes matched by relative path, with notes for partial scans and different folders
- [x] M6.8 Mac multiple windows: the app's `Window` is now a `WindowGroup` with one `ScanWorkspace` per window (`SpaceApp/WorkspaceCommands.swift`), **File → New Window** (⌘N) appears with Pro features on, and the menus act on the focused window through a focused scene value. Automatic window tabbing is off, so Basic stays one window. The menu bar's **Open Space 16X** brings back an open window before opening one.
- [x] M6.9 Promo refresh (Pro's `scan --cleanup` in the CLI and the Mac app's Duplicates, 29 s, and a new README preview from the larger fixture), manual QA rows for every M6 feature, benchmarks and both editions' archive sizes in `docs/qa.md`, and the **Later** list
- [x] M6.10 Release v0.8.0

### M7 (v0.9.0) Complete and stable [Basic + Pro]
The owner's v0.8.0 QA found that every Pro feature works but is easy to miss, and that duplicates overstate what removing a clone frees. M7 fixes that and finishes what M6 cut.
- [x] M7.1 Fixes from the v0.8.0 QA:
  - Duplicates become clone-aware in the CLI and the Mac app. A copy that shares its blocks with another copy in the group (an APFS clone, or a Btrfs/XFS reflink) is marked as a clone that frees nothing, and **Extra** counts only the bytes that removing the other copies would free. Reuse the exclusive-size measurement (`cli/src/pro/exclusive.rs`, `SpacePro/ExclusiveSize.swift`) and test both apps with the same case.
  - Basic `volumes` hides mount points that aren't directories, such as files bind-mounted into a container (`/etc/hostname`), unless `--all` is given.
  - Pro's `--help` lists the explorer's `m` treemap key.
- [x] M7.2 Make Pro easy to find [Basic]:
  - **Mac:** while Pro features is off, show one quiet, dismissible line, for example on the start screen: "More views are free in Pro". It opens **Settings → Pro features**.
  - **README:** a short **Editions** section with a Basic/Pro table and how to turn on each, in place of the long paragraph.
  - **Release notes:** each GitHub release includes its version's `CHANGELOG.md` section above the standard text (`publish-cli.yml`).
- [x] M7.3 Exclusive size as a column [Pro]:
  - **CLI explorer:** a key turns on an **Exclusive** column.
  - **Mac:** a sortable **Exclusive** column, shown from the View menu.
  - Both measure after the scan, in the background and only when asked, with progress. The walk stays as fast as before.
- [x] M7.4 Duplicates and Explain views in the CLI explorer [Pro]:
  - Pick keys that don't clash with existing ones (`d` trashes and `x` cancels) and list them in `?` help.
  - Duplicates reads file contents only after the user starts it in that view. Its groups support marking and the confirmed Move to Trash.
  - Explain shows the open folder's volume.
- [x] M7.5 APFS snapshot detail [Pro], about a day at most:
  - Look for a supported way to size each local snapshot, such as `tmutil`, `diskutil apfs` or a privilege the CLI may already run with. If one exists, show each snapshot's size in `explain` and the Mac sheet.
  - Otherwise, show each snapshot's date and how macOS frees snapshots, and record here why per-snapshot sizes stay out.
  - No private APIs and no deletion.
  - **Outcome (2026-11-22):** there's no supported way. `tmutil listlocalsnapshots` and `diskutil apfs listSnapshots` give only names, XIDs and whether a snapshot is purgeable, never a size. `fs_snapshot_list` needs root, and sizing a snapshot otherwise means mounting it (root) and walking it file by file, as `tmutil uniquesize` would. So `explain` and the Mac sheet list each snapshot's date, oldest first, with how macOS frees them (after about 24 hours, or sooner when space runs low), and per-snapshot sizes stay out.
- [x] M7.6 Size budgets for Build Footprint [Pro]:
  - **CLI:** `diff` and `history diff` take `--budget SIZE`, with a new, documented exit status when growth exceeds it (versioned in `docs/cli-pro.md`).
  - **Mac:** the receipt sheet takes an optional budget, remembered per operation name, and the receipt says whether the operation stayed within it.
- [x] M7.7 "Review" cleanup suggestions [Pro]:
  - Add Xcode Archives and simulator devices from the shared rule table, in both apps, as a separate **Review** kind: they can't be recreated.
  - Review items stay out of the recreatable total and are never pre-marked.
  - Move to Trash warns plainly before moving them.
- [x] M7.8 Mac parity [Pro]:
  - An older-than filter, like `scan --older-than`: **View → Show Files Older Than…** with a few presets.
  - **Export HTML Report…**, writing the same self-contained page as `scan --html`, with a shared structural test.
  - Owners in Scan Details if that's simple. Otherwise, record why the Mac app leaves owners out.
  - **Outcome (2026-11-22):** **View → Show Files Older Than** (Any Age, 30 and 90 days, 6 months, 1 or 2 years) filters the results with `SpacePro/OlderThan.swift`, the CLI's rule. Saving, exporting a scan, rescanning and Move to Trash work from the unfiltered results, Move to Trash refuses folders while the filter is on (they may hold newer files), and Compare is off while it's on. **File → Export HTML Report…** writes `SpacePro/HTMLReport.swift`'s port of the CLI page, and `fixtures/html-report` checks both pages' structure. **Owners stay out of the Mac app:** it scans the signed-in user's folders, where nearly every file has one owner, so a breakdown would say little, and storing each entry's owner would grow `ScanEntry` past its 64 bytes. The CLI's `--by-owner` serves shared servers.
- [x] M7.9 QA, benchmarks, archive sizes and a promo refresh:
  - Exercise every Mac feature M7 touches at runtime, not only in offscreen renders. A temporary debug build that drives `ScanWorkspace` and logs, run with `-proFeatures YES` argument overrides, works well; don't commit it.
  - Add manual QA rows, and list what still needs the owner's eyes.
  - **Outcome (2026-11-22):** `docs/qa.md` records the runtime pass, which found and fixed two layout bugs. **Still for the owner's eyes:** clicking the start screen's **Turn On in Settings…** and its × (relaunch to see it stay hidden); typing in the receipt sheet (a remembered budget filling in, and **Save…** disabled for an unreadable budget); saving through the HTML report and receipt save panels (the harness checked the panels and the files' contents, not a click on **Save**); a real Time Machine local snapshot in Explain Disk Space; the View menu's **Show Files Older Than** submenu; and the Move to Trash alert's look. The console warning about a reentrant table delegate when the age filter was cleared or the Exclusive sort reversed was fixed on `main` after the release.
- [x] M7.10 Release v0.9.0

### Future
- **Cross-platform (owner to plan).** After v0.9.0, the owner will review whether the Rust core can carry a desktop app beyond macOS. Don't start this work until the owner has reviewed feasibility and approved a plan.

## Working agreement

- **Small PRs.** Each roadmap item lands as its own branch and pull request (`feature/m1-…`). Commit and push often, so work survives an interrupted session. Tick the item here in the same PR.
- **Merge** only after CI passes, locally with `git merge --no-ff` and a push, never the GitHub merge button or `gh pr merge`. Commits use the identity `muhammad-naveed <m.naveed.ch@gmail.com>`. The admin-bypass notice on pushes to `main` is expected. Run the merge, the push and the branch deletion as one `set -e` sequence, and delete the branch only after `gh pr view N --json state` says `MERGED`. A failed merge followed by a branch deletion closes the PR unmerged.
- **Parallel branches** that tick neighboring checkboxes here conflict on merge. Keep every tick when resolving.
- **Release** when a milestone's last item lands and CI on `main` is green: a `release/vX.Y.0` PR that bumps `scripts/release-version`, `cli/Cargo.toml`, `cli/Cargo.lock`, `CHANGELOG.md` and `docs/install.md` (as in commit `chore: release 0.2.4`), merged the same way, then push the `vX.Y.0` tag. **Publish release** builds and publishes it. Check that run succeeds.
- **CI** grows with the feature set but stays simple: extend `scripts/check-cli.py`, `scripts/check-tui.py` and the unit tests before adding jobs. Add a job only for something the existing jobs can't cover. If a CI job is renamed or added, update the required checks in the **Review and CI for main** ruleset too.
- **Rules from `AGENTS.md` always apply:** read-only, metadata-only scanners, both scanners kept in parity, the CLI contract in `docs/cli.md`, and a promo refresh for major UI changes.

## How to resume

A scheduled task (`space16x-roadmap` in the Claude desktop app) starts a session every 6 hours to continue this roadmap. An interactive session follows the same steps.

0. **Lock.** If `.git/space16x-roadmap.lock` was modified less than 120 minutes ago, another session is working: stop. Otherwise write the current date into it, refresh it after every commit, and delete it when you stop. A session cut off by a usage limit leaves a lock that goes stale on its own.
1. `git fetch` and read this file's **Progress** and **Resume here**.
2. Look for unfinished work: open PRs (`gh pr list`), local and remote `feature/*` branches, and uncommitted changes. Continue that before starting the next item.
3. If a PR's CI failed, fix it first. If it passed, merge it and tick the item.
4. Update **Resume here** whenever you stop partway through an item: the branch, what's done, and the next step. Commit and push that note.

## Context

Space 16X (v0.2.4) is a read-only disk analyzer with two products: a SwiftUI Mac app (`Sources/SpaceApp` + `Sources/SpaceCore`) and a Rust CLI/TUI (`cli/`) for macOS and Linux. The aim is to add the features users ask for most without losing what makes it good: a simple UI, robust performance, and a CLI light enough for remote servers.

The answer is **two free, open-source editions built from this one repository**:
- **Basic** is lightweight and focused.
- **Pro** is feature-rich.

This file covers the competitor scan, sourced user gaps, the edition split and its architecture, the roadmap, and a detailed first milestone.

**Decisions so far (2026-11-21):**
- **Everything is free and MIT, in this repo.** There is no paid tier, no private repo, and no licensing or payment code.
- **CLI:** Basic and Pro are separate builds. Both install the **same `space16x` command**, so a user installs one or the other and the installer switches between them.
- **Mac:** one app at a time. Recommendation (§4): **one "Space 16X" app with a Pro features switch** rather than two installs.
- **Move to Trash** is in Basic: Trash only, never permanent delete.

## What exists today (don't re-propose)

- **CLI:** `space16x [PATH]` opens the TUI. `scan [PATH] --depth N | --top N [--json]` writes JSON schema 2. Exit codes are 0, 2 and 1.
- **TUI:** browse, `/` filter, `f` largest files (100), `e` issues, file details, `c` cancel keeping the preview, progressive preview, protected paths.
- **Mac app:** list, treemap, donut, Largest Files, Scan Details, reveal and copy path, ⌘R rescan, drag and drop, Full Disk Access guidance. It follows the system light/dark appearance.
- **Accounting:** allocated bytes. Hard links counted once. Symlinks and other-device mounts skipped. APFS firmlinks counted once. Cloud placeholders count as zero. Partial results are explicit.
- **Gaps in the code:**
  - The Rust walk is single-threaded (`cli/src/scanner.rs:240`).
  - Every node stores its full absolute path (Rust `Entry.path`, Swift `ScanEntry.path`).
  - There are no excludes and no apparent size in Rust. `SizeMetric.logical` exists in Swift but the UI never uses it.
  - There are no recursive item counts, sort options, export/import or config.
  - Swift models have no public initializers and are not `Codable`.
- **Footprint:**
  - Benchmark: 100K files in about 0.19 s, with about 34.5 MB peak RSS (`docs/qa.md`).
  - Release CLI archives are about 450 KB, with 6 dependencies.

## 1. Competitor scan (GitHub stars checked live with `gh api`, 2026-11-21)

Only six relevant tools have **more than 10K stars**. gdu, dua, WinDirStat and the other well-known analyzers are below 10K, and ncdu and GrandPerspective are not hosted on GitHub. They are listed anyway because users cite them constantly.

| Tool | Stars | License | What it offers | Fits Space 16X as |
|---|---:|---|---|---|
| [tw93/Mole](https://github.com/tw93/Mole) | 75,710 | GPL-3.0 | Mac/Win cleaner CLI. `mo analyze` TUI with filter, multi-select, preview and confirmed Trash. JSON `scan_status`. `purge` for build artifacts. A separate paid native app. | Confirmed Trash: **Basic**. Artifact cleanup: **Pro**. |
| [qarmin/czkawka](https://github.com/qarmin/czkawka) | 35,361 | MIT core/CLI, GPL-3 GUI | Duplicates, empty folders, big files, similar media. Core library + CLI + GUI. | Duplicates: **Pro**. Its layout (core + frontends) is a model. |
| [jarun/nnn](https://github.com/jarun/nnn) | 22,132 | BSD-2 | File manager with du and apparent-size modes. | Apparent size: **Basic**. |
| [muesli/duf](https://github.com/muesli/duf) | 15,396 | MIT | df replacement: mounts, used/free, inodes, JSON. | `volumes`: **Basic**. |
| [Canop/broot](https://github.com/Canop/broot) | 13,173 | MIT | Tree navigator. Whale-spotting mode computes sizes, dates and counts in the background. | Counts: **Basic**. Dates: **Pro**. |
| [bootandy/dust](https://github.com/bootandy/dust) | 12,669 | Apache-2.0 | `-X` ignore, `-x`, `-s` apparent, `-f` counts, `-t` types, `-T` threads, `-j` JSON, config. | Excludes, apparent, counts, threads: **Basic**. Types: **Pro**. |
| [voidcosmos/npkill](https://github.com/voidcosmos/npkill) | 9,614 | MIT | Finds and deletes `node_modules`. | **Pro** smart cleanup. |
| [Byron/dua-cli](https://github.com/Byron/dua-cli) | 6,458 | MIT | Parallel walk. Mark → trash/delete. `--ignore-from`. `dua clean`. Flame graphs. | Mark + Trash, ignore file: **Basic**. |
| [dundee/gdu](https://github.com/dundee/gdu) | 6,215 | MIT | Parallel walk. ncdu-format export/import. `--db`. Age filters. `--exclusive-size`. `--no-delete`. Web UI. | Export/import, `--read-only`: **Basic**. Age, exclusive size: **Pro**. |
| [windirstat/windirstat](https://github.com/windirstat/windirstat) | 4,585 | GPL-3.0 | Treemap, extension list, duplicates, cleanup, monitoring (Windows only). | Types, duplicates: **Pro**. |
| [imsnif/diskonaut](https://github.com/imsnif/diskonaut) | 3,163 | MIT | Terminal treemap. Last push 2024. | Its top issue is excludes. Terminal treemap: **Pro**. |
| [shundhammer/qdirstat](https://github.com/shundhammer/qdirstat) | 2,670 | GPL-2.0 | Treemap, type and age statistics, cleanup actions. | Age, types: **Pro**. |
| ncdu ([dev.yorhel.nl](https://dev.yorhel.nl/ncdu)) | not on GitHub ([HN, 446 pts](https://news.ycombinator.com/item?id=33893815)) | MIT | Export/import (`ssh … ncdu -o- / \| ncdu -f-`), threads, excludes, config, read-only mode, item-count sort. | Reference for the Basic server workflow. |
| GrandPerspective ([SourceForge](https://grandperspectiv.sourceforge.net/)) | not on GitHub | GPL | Treemap. | Covered by the existing treemap. |
| Filelight ([KDE mirror](https://github.com/KDE/filelight)) | 302 (mirror) | GPL | Sunburst chart. | Covered by the existing donut. |

## 2. User-requested features Space 16X lacks

| Need | Evidence | Edition, milestone |
|---|---|---|
| Exclude folders/patterns | [diskonaut #80](https://github.com/imsnif/diskonaut/issues/80) (22 👍, its top issue), [#93](https://github.com/imsnif/diskonaut/issues/93), [dua #30](https://github.com/Byron/dua-cli/issues/30), [WinDirStat #99](https://github.com/windirstat/windirstat/issues/99), [HN](https://news.ycombinator.com/item?id=33895742) | Basic, **M1** |
| Item/inode counts, sort by count | [dua #144](https://github.com/Byron/dua-cli/issues/144), [raymii: inode exhaustion, ncdu `C`](https://raymii.org/s/articles/ncdu_-_for_troubleshooting_diskspace_and_inode_issues.html) | Basic, **M1** |
| Apparent/logical size | [WinDirStat #258](https://github.com/windirstat/windirstat/issues/258), [diskonaut #56](https://github.com/imsnif/diskonaut/issues/56) | Basic, **M1** |
| No-color output for limited terminals and servers; dark mode | [WinDirStat #106](https://github.com/windirstat/windirstat/issues/106) (29 👍) asks for dark mode, which the Mac app already has | Basic, **M1** `NO_COLOR` |
| Trash from the tool, plus a switch to disable it | [gdu #12](https://github.com/dundee/gdu/issues/12), [gdu #224](https://github.com/dundee/gdu/issues/224), [HN](https://news.ycombinator.com/item?id=40933010) | Basic, **M2** |
| Rescan/refresh a folder | [dua #96](https://github.com/Byron/dua-cli/issues/96) (14 👍, its top issue) | Basic, **M2** |
| Faster scans, low memory | [HN: WizTree speed](https://news.ycombinator.com/item?id=33903047), [gdu vs ncdu benchmark](https://linuxuprising.com/2021/01/gdu-is-fast-console-disk-usage-analyzer.html?m=0) | Basic, **M3** |
| Save/load scans; scan remotely, browse locally | [dua #137](https://github.com/Byron/dua-cli/issues/137), [gdu #141](https://github.com/dundee/gdu/issues/141), [HN](https://news.ycombinator.com/item?id=27949848), [ncdu man page](https://manpages.debian.org/trixie/ncdu/ncdu.1.en.html) | Basic, **M4** |
| Which mount is full | duf (15K★), `gdu -d`, [Linode](https://www.linode.com/community/questions/16922/my-disk-is-filling-up-how-can-i-find-the-culprit) | Basic, **M4** |
| "What grew since last week?" | [HN](https://news.ycombinator.com/item?id=40938244), PRD Build Footprint | Pro, **M4** diff → **M5** history |
| Age/mtime filter | [dust #340](https://github.com/bootandy/dust/issues/340), `gdu --max-age` | Pro, **M5** |
| Breakdown by file type | [WinDirStat #87](https://github.com/windirstat/windirstat/issues/87), `dust -t`, [HN Leaves](https://news.ycombinator.com/item?id=48938098) | Pro, **M5** |
| Who uses a shared server; shareable reports | [HN](https://news.ycombinator.com/item?id=48954816) | Pro, **M5** owner, **M6** HTML |
| What deleting actually frees (APFS clones, reflinks) | [dust #590](https://github.com/bootandy/dust/issues/590), [HN](https://news.ycombinator.com/item?id=40932959), `gdu --exclusive-size` | Pro, **M6** |
| macOS "System Data", snapshots, df≠du | [Apple Community](https://discussions.apple.com/thread/254856441), [Red Hat: deleted-open files](https://access.redhat.com/solutions/2316) | Pro, **M6** |
| Duplicates; developer-artifact cleanup | czkawka 35K★, dupeGuru 8K★, npkill 9.6K★, `dua clean`, `mo purge` | Pro, **M6** |

## 3. The edition rule

- **Basic** answers one question: *what is using my space, and can I get rid of it?*
- **Pro** adds the features that cost something. A feature belongs in **Pro** if it:
  - adds per-node memory beyond the core fields;
  - adds a dependency;
  - keeps state on disk;
  - runs in the background;
  - reads file contents;
  - or adds significant UI surface.

| Basic (default; lightweight) | Pro (adds) |
|---|---|
| Scan, browse, filter, largest files, issues | `diff` of two scans (CLI); Compare on Mac |
| Excludes, item counts, sort modes, apparent size, `NO_COLOR` | Scan history: CLI stores scans under XDG state; Mac has a saved-scan library |
| Move to Trash (confirmed), rescan folder, `--read-only` | Age/mtime column and `--older-than`; file-type breakdown; owner (uid) breakdown |
| Parallel walk, lean memory | Clone-aware exclusive size; opt-in duplicate finder that hashes contents on explicit request only |
| Export/import (ncdu format), `volumes` | Explain my disk: APFS snapshots and purgeable space on Mac; deleted-but-open files on Linux |
| JSON schema 3, exit codes 0, 2, 1 | Smart cleanup suggestions (DerivedData, Simulators, `node_modules`, `target`, caches) that feed Basic's Trash flow |
| | HTML report; terminal treemap |
| | **Mac only:** menu-bar free-space monitor, low-space alerts, Build Footprint receipts, multiple windows |

## 4. Architecture: one repo, two editions

**Rust CLI: one crate with a Cargo feature.**
- **Feature flag:** `cli/Cargo.toml` gets `[features] pro = [...]`. Pro-only dependencies, such as a hash crate, are `optional = true` and turned on only by `pro`. The binary name stays `space16x`.
- **Code layout:** Pro code lives under `cli/src/pro/` behind `#[cfg(feature = "pro")]`. Basic code has only a few hook points:
  - subcommand dispatch;
  - the TUI key and column registry;
  - help text.
- **Side tables:** Pro per-node data (mtime, uid, type) lives in **side tables keyed by node index** on M3's arena tree. Basic's memory never pays for Pro fields. So the lean tree (M3) comes **before** any Pro feature that needs per-node data.
- **Basic hints:** in a Basic build, a Pro subcommand such as `space16x diff` prints a one-line hint to install Pro and exits 1.
- **Version string:** `space16x --version` shows `space16x 0.x.y` or `space16x 0.x.y (pro)`.
- **JSON:** schema 3 adds `"edition": "basic" | "pro"`. Pro-only fields are optional and documented in a Pro section of `docs/cli.md`.
- **Releases:** archives `space16x-<target>.tar.gz` (Basic) and `space16x-pro-<target>.tar.gz`, built from the same tag for all 4 targets.
- **Installer:** `scripts/install.sh --pro` (Basic is the default) installs into the same path, so installing one edition replaces the other. `--uninstall` removes whichever is present. Re-pin the README one-liners and run `scripts/check-install.py`.
- **Budgets enforced in CI:**
  - **Basic:** archive ≤ 600 KB, no new dependencies beyond the Trash backend, and per-node RSS no higher than after M3.
  - **Pro:** dependencies only after a `cargo deny` review.

**Mac: recommend one app, "Space 16X", with Settings → Pro features (off by default).**
- **Why one app:**
  - On a Mac, binary size isn't the constraint (the app zip is about 2 MB). *Lightweight* means a simple UI, and an off-by-default switch gives Basic users exactly the Basic UI.
  - One app means one bundle ID, one cask, one Full Disk Access grant and one set of settings. Switching modes needs no reinstall. Nobody ends up with two similar apps, which the owner flagged as confusing.
  - It halves packaging, QA and promo work compared with two exclusive builds.
- **What Pro mode adds:** extra views (Compare, History, Types, Age), a Modified column, and Explain/Cleanup panels. The menu-bar monitor and background snapshots start only when they are enabled.
- **Swift package:**
  - A new `SpacePro` library target holds Pro analysis (diff, history store, aggregations, snapshot queries). It depends on `SpaceCore`, and `SpaceApp` links both.
  - The UI is gated by `@AppStorage("proFeatures")`.
  - `SpaceCore` gains `Codable` and `Sendable` models with public initializers for export/import and history (M4).
- **Fallback if you'd rather have separate installs:** two builds with the same bundle ID and casks `space16x` and `space16x-pro` that declare `conflicts_with`. It costs more and isn't recommended.

**Rules to update in `AGENTS.md`, `PRD.md`, `README.md` and `SECURITY.md`:**
- Scanners stay read-only and metadata-only.
- The only write is a user-confirmed Move to Trash, from M2.
- Reading file contents (Pro duplicates) happens only on an explicit user action, never during a scan.
- Every change builds and tests both editions.
- Replace the PRD's "macOS premium hypothesis" and pricing section with the edition model.

**Licensing:**
- Everything stays MIT in one repo, so contributions are inbound = outbound and no CLA is needed.
- Take ideas, not code, from GPL/AGPL projects (Mole, WinDirStat, QDirStat, dupeGuru, Krokiet, SquirrelDisk).
- Pro dependencies must pass the existing `cargo-deny` license allowlist.

## 5. Roadmap

Effort: S is about 1 day, M is 2–4 days, L is 1–2 weeks. Each milestone is one minor release, synced through `scripts/release-version`.

- **M1 (v0.3) Focus [Basic].** Excludes, item counts, sort modes, apparent size, `NO_COLOR`, JSON schema 3 (including `edition`). *Detailed in §6.*
- **M2 (v0.4) Act [Basic].**
  - **Move to Trash, Mac (M):**
    - Multi-select, then ⌘⌫, using `FileManager.trashItem`.
    - A confirmation sheet shows the item count and size.
    - The tree updates in place.
    - Cloud-only placeholders and protected items are blocked.
  - **Move to Trash, TUI (M):**
    - Space marks items. `d` trashes after a `y/N` prompt.
    - `--read-only` and `SPACE16X_READ_ONLY=1` turn trashing off.
    - **Linux backend:** a small in-house FreeDesktop Trash, using `rename(2)` into the home Trash or `$topdir/.Trash-$uid` and writing `.trashinfo` first. It refuses to cross devices rather than copy.
    - **macOS backend:** `trashItemAtURL`. Pick the smaller of a minimal binding and the `trash` crate.
  - **Messaging:** "Space is freed when you empty the Trash." Trashing hard links, clones or snapshotted files may free nothing.
  - **Rescan folder:** TUI `r` and Mac "Rescan Folder" (M).
  - **Quick Look** on Space in the Mac app (S).
  - **Promo refresh.**
- **M3 (v0.5) Speed [Basic] (L).**
  - **Lean tree:** an arena of name-only nodes with parent indices, which also supplies the node indices for Pro side tables. Swift drops stored full paths.
  - **Parallel walk:** `--threads N` (default `min(cores, 8)`; `1` for HDDs and NFS), using std threads and a work queue.
    - Hard-link dedup uses a sharded set.
    - Output stays deterministic, with ties broken by path.
    - Swift uses a TaskGroup walk.
  - **Gates:** `scripts/benchmark.py` gains a 1M-file option. Target at least 2× faster on SSD and no RSS growth per node.
- **M4 (v0.6) Carry, compare, editions.**
  - **Basic:**
    - **Export/import:** `scan --export FILE|-` writes ncdu JSON v1 ([format](https://dev.yorhel.nl/ncdu/jsonfmt)) with `space16x_*` keys, so ncdu and gdu files open both ways. `space16x --import FILE|-` opens the TUI, which enables `ssh -C host space16x scan / --export - | space16x --import -`.
    - **Mac:** Open Scan / Export Scan.
    - **Volumes:** `space16x volumes` (used, free, inodes via `statvfs`) and a volume list on the Mac start screen.
  - **Edition scaffolding:** the `pro` feature, release archives for both editions, the installer's `--pro`, CI for both, and the Mac Pro features switch.
  - **First Pro feature:** `space16x diff OLD NEW|PATH [--top N] [--json]` and Mac Compare.
  - **Promo refresh.**
- **M5 (v0.7) Pro analysis (M).**
  - Newest-mtime column and `--older-than`.
  - File-type breakdown (TUI `t`, `scan --types N`).
  - `scan --by-owner`.
  - Scan history: `space16x history` and the Mac saved scans, both building on diff.
- **M6 (v0.8+) Pro deep (L).**
  - Explain my disk.
  - Smart cleanup suggestions.
  - Exclusive size.
  - Opt-in duplicates.
  - HTML report and terminal treemap.
  - Mac menu-bar monitor and alerts, Build Footprint receipts, multiple windows.
- **M7 (v0.9) Complete and stable (M).** Fixes from the owner's v0.8.0 QA (clone-aware duplicates, Pro discoverability, release notes from the changelog), then what M6 cut: an exclusive-size column, duplicates and explain in the explorer, snapshot detail, footprint budgets, "review" cleanup suggestions, and Mac parity for age filters and the HTML report. *Items in Progress.*

## 6. Milestone 1 (v0.3.0) in detail: Focus [Basic]

**Scope.** Every item is additive and read-only, and ships in both the CLI and the Mac app.

1. **Excludes.**
   - **CLI:** `--exclude PATTERN` (repeatable) and `--exclude-from FILE`, for both `scan` and the TUI.
   - **Patterns:** a gitignore-like subset: `*`, `?` and `**`. A leading `/` anchors to the scan root; otherwise the pattern matches any segment.
   - **Implementation:** hand-written, about 150 lines in each scanner, with no `globset` or `regex`.
   - **Accounting:** excluded entries are expected exclusions, so the scan stays complete (exit 0). They are summarized on stderr and in JSON (`excludedCount`, `excludes`).
   - **Mac:** a new Settings scene listing patterns, stored in UserDefaults.
2. **Item counts.** `itemCount` counts every entry below a directory within the scan, with hard links counted once and excluded entries left out. It appears as an Items column in the TUI (when the width allows) and in the Mac table.
3. **Sorting.**
   - **TUI:** `s` cycles size → items → name.
   - **Mac:** sortable Table columns.
   - Ties are ordered by path bytes.
4. **Apparent size.**
   - **Rust:** stores `st_size` from the existing `fstatat` call.
   - **CLI/TUI:** `scan --apparent`; TUI `a` toggles it.
   - **Mac:** View → Size: Allocated / Logical, using the existing `SizeMetric.logical`. The treemap and donut follow the choice.
5. **`NO_COLOR` and `--no-color`** in the TUI.
6. **JSON schema 3.** Schema-2 fields keep their meaning. It adds `apparentBytes`, `itemCount`, `excludedCount`, `excludes`, `rankedBy` and `edition`. Exit codes don't change. Regenerate `docs/example-report.json`.

**Critical files.**
- **Rust:**
  - `cli/src/scanner.rs`: `Entry` and the walk.
  - `cli/src/scanner/fs.rs`: `st_size`.
  - New `cli/src/scanner/exclude.rs`.
  - `cli/src/main.rs`: arguments.
  - `cli/src/report.rs`: schema 3.
  - `cli/src/tui/explorer.rs`: keys and help.
  - `cli/src/tui/ranker.rs`: sort comparators.
  - `cli/src/tui/view.rs`: Items column, header, no-color.
- **Swift:**
  - `Sources/SpaceCore/DiskScanner.swift`.
  - New `Sources/SpaceCore/ExcludeRules.swift`.
  - `Sources/SpaceCore/ScanReport.swift`.
  - `Sources/SpaceApp/ResultsView.swift`: Items column and `sortOrder`.
  - `Sources/SpaceApp/ScanWorkspace.swift`: sort and metric state, ranking at `:337`.
  - `Sources/SpaceApp/Space16XApp.swift`: Settings scene and View menu.
  - New `Sources/SpaceApp/SettingsView.swift`.
- **Docs:** `docs/cli.md`, `README.md`, `CHANGELOG.md`, `docs/manual-qa.md`, `docs/qa.md`. Also `PRD.md` and `AGENTS.md` (edition model and rules), and the `scripts/promo/` storyboard.

**Reuse.**
- Rust `Fixture` (`cli/src/scanner/tests.rs:8-46`).
- TUI builders `file()`, `folder()`, `show()`, `key()` (`cli/src/tui/explorer.rs:489-529`).
- Swift `withFixture`, `makeFile`, `allocatedBytes(of:)` (`Tests/SpaceCoreTests/DiskScannerTests.swift:390-429`).
- `scripts/check-cli.py`, with its `run(status=)` and fixture tree.
- The pty harness in `scripts/tui_terminal.py`.
- `SizeMetric` and `rankedChildren(by:)`.
- The tie ordering at `ranker.rs:100-111`.

**Done when:**
- One shared pattern table and fixture produce identical allocated bytes, apparent bytes, item counts, excluded counts and rankings in the Rust and Swift scanners. The fixture includes hard links, a sparse file, a symlink, a `chmod 0` directory and nested matches.
- Exit codes are unchanged.
- The 100K benchmark's RSS rises by **20% or less**. The 1M-file result is recorded.
- The archive stays **600 KB or smaller**.
- The promo is refreshed: the storyboard shows the Items column with sort and the size toggle.

## Verification (each milestone)

- **Rust, both editions (from M4):**
  - `(cd cli && cargo fmt --all --check && cargo clippy --locked --all-targets -- -D warnings && cargo test --locked)`.
  - The same `clippy` and `test` with `--features pro`.
  - `python3 scripts/check-cli.py <cli>` and `python3 scripts/check-tui.py <cli>` against each release binary.
- **Swift:** `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer swift test` and `scripts/package-macos.sh`, plus the new rows in `docs/manual-qa.md`: excludes, sorting, the size toggle, later Trash with Put Back, and the Pro switch.
- **Performance:** `python3 scripts/benchmark.py <cli>` at 100K and 1M files, recorded in `docs/qa.md`, along with the archive size of each edition.
- **Linux:** CLI checks as a standard user on x86-64 and ARM64. From M2, FreeDesktop Trash and refusal across devices. From M4, the installer switching editions (`scripts/check-install.py`).
- **Promo:** run the generator on macOS, review the MP4, refresh `Branding/Space16X-Demo.gif`, and include "New promo ready to publish" in the handoff.

## Risks and open items

- **Pro code paths could rot.** If `cfg` sprawl creeps in, keep Pro behind a handful of hooks and test both editions in CI on every PR.
- **Trash on a Linux server frees nothing until the Trash is emptied.** Say so plainly. Add a permanent-delete option only if users ask, and only as an explicit opt-in.
- **Schema 3 breaks consumers pinned to `schemaVersion == 2`.** The project is 0.x and the change is additive, so document it in the changelog.
- **The scanners keep diverging.** Every shared change ships with the same fixture table in both test suites.
