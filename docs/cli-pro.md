# Space 16X Pro CLI reference

Space 16X Pro is a free edition of the CLI with more analysis. It has every Basic command and option in the [CLI reference](cli.md), which also defines the exit codes and JSON schema 3 that these commands follow, and adds the commands below. Install it with `install.sh --pro`, as the [installation guide](install.md#install-space-16x-pro) shows. JSON reports from Pro say `"edition": "pro"`, and a Basic build answers these commands and options with a one-line hint to install Pro.

## Compare scans with diff

```text
space16x diff OLD NEW|PATH [--depth N | --top N] [--apparent] [--json] [--threads N]
```

`space16x diff` shows what grew or shrank. OLD is a saved scan: a `scan --export` file, an ncdu or gdu export, or `-` for standard input. NEW is another saved scan, or a folder to scan now. A folder scan uses OLD's exclude patterns and `--threads`, so both sides count the same things. Entries match by their path below each root, so the roots can differ, as for a server's export compared with a backup.

```sh
space16x scan ~ --export ~/home-monday.json
space16x diff ~/home-monday.json ~
space16x diff ~/home-monday.json ~ --top 10
ssh server space16x scan /srv --export - | space16x diff - srv-last-week.json
```

```text
     Change        Old        New  Path
   +12.4 GB    20.0 GB    32.4 GB  /Users/me/Library
    +2.1 GB          -     2.1 GB  /Users/me/Videos  [added]
    -1.2 GB     3.0 GB     1.8 GB  /Users/me/Downloads
   +13.3 GB   420.0 GB   433.3 GB  /Users/me  [total]
```

| Option | Behavior |
| --- | --- |
| `--depth N` | List changed entries through N levels, each folder's changes ranked by size of change. The default is one level. Folders whose total didn't change aren't expanded. |
| `--top N` | List the N files that changed the most, either way, including files only one scan has. |
| `--apparent` | Compare apparent sizes instead of allocated bytes. |
| `--json` | Write schema 3 with `schemaVersion`, `edition`, `rankedBy`, `old` and `new` summaries (`scope`, `allocatedBytes`, `apparentBytes`, `itemCount`, `isPartial`), `changeBytes`, and either `depth` and `entries` or `topLimit` and `topFiles`. Each change has `path`, `kind`, `oldBytes`, `newBytes` (`null` when that scan doesn't have it), and a signed `changeBytes`. |
| `--threads N` | Threads for scanning NEW when it's a folder. |

Entries only one scan has are marked `[added]` or `[removed]`. The exit code is `0`, `2` when either scan is partial, and `1` for errors.

## Modification times and --older-than

Pro reads each file's modification time with the same metadata call that reads its size, so scans take no longer. A folder's time is the **newest** modification time of any file below it, which tells you when anything inside last changed. A folder without files has no time.

- The explorer adds a **Modified** column with each item's newest modification date, in local time, when the terminal is at least 96 columns wide. File details show the date too. Imported scans don't carry times, so the column stays hidden for them.
- JSON reports add `newestModified` to the root, each entry and each top file, in Unix seconds.

```text
space16x scan [PATH] --older-than DAYS [--depth N | --top N] [--json] ...
```

`--older-than DAYS` counts only files last modified more than DAYS days ago, and the folders that hold them. Sizes, item counts, rankings and top files then describe that older data alone, so `space16x scan ~ --older-than 365` shows where the files you haven't touched in a year are. Folders without such files are left out, except folders that couldn't be read completely, which stay so their state still shows. Text reports add a line on standard error with the cutoff date, and JSON reports add `olderThanDays`. `--export` still saves the whole scan. Times before 1970 count as 1970.

```sh
space16x scan ~/Projects --older-than 180
space16x scan /srv --older-than 365 --top 20 --json
```

## File types

```text
space16x scan [PATH] --types N [--apparent] [--older-than DAYS] [--json] ...
```

`--types N` lists the N file types that use the most space in the scan, with their size and file count, in place of entries. The rest are summed in one line, and the last line is the scan's total. Press `t` in the explorer for the same list for the open folder, ranked by the current size, and `t` or Esc to close it.

```text
      Size      Files  Type
  48.12 GB        214  .mov
   6.30 GB     12,480  .jpg
   1.02 GB      3,911  (no extension)
   2.75 GB     40,177  (38 other types)
  58.19 GB     56,782  /Users/me/Pictures  [total]
```

A file's type is the text after the last dot of its name, in lowercase for ASCII letters, so `Clip.MOV` and `clip.mov` are both `.mov`, and `backup.tar.gz` is `.gz`. A name that starts with a dot, such as `.bashrc`, has no extension, and neither does a name whose text after the dot is empty, contains a space, or is longer than 16 bytes, as in `report.2026-11-21T10-00-00`. Each hard-linked file counts once, at the path the scan counts it. `--types` takes no `--depth` or `--top`.

JSON reports put `typeLimit`, `typeCount` (the number of distinct types in the scan) and `types` in place of `depth` and `entries`. Each type has `type` (the extension without its dot, or an empty string), `allocatedBytes`, `apparentBytes` and `fileCount`. `--apparent` ranks types by apparent size, and `--older-than` counts only older files.

## Owners

```text
space16x scan [PATH] --by-owner [--apparent] [--older-than DAYS] [--json] ...
```

`--by-owner` lists the space and the files and folders each user owns, largest first, in place of entries, which shows who fills a shared server. Run it as a user who can read the folders, such as root, to count everyone's files.

```text
      Size      Items  Owner
 412.30 GB    981,204  alice
  96.02 GB    120,388  postgres
   3.10 GB      1,004  1007
 511.42 GB  1,102,596  /srv  [total]
```

Owners come from the user database. The Linux release builds are static and read `/etc/passwd` only, so users from a directory service such as LDAP show their numeric ID. A folder counts as an item of its owner but has no bytes of its own, and each hard-linked file counts once, for the owner of the path the scan counts. `--by-owner` takes no `--depth`, `--top` or `--types`.

JSON reports put `owners` in place of `depth` and `entries`. Each owner has `owner` (the user name, or `null` without one), `uid`, `allocatedBytes`, `apparentBytes` and `itemCount`.

## Cleanup suggestions

```text
space16x scan [PATH] --cleanup [--apparent] [--older-than DAYS] [--json] ...
```

`--cleanup` lists folders that tools recreate when they need them, in place of entries, largest first, with the total. Press `u` in the explorer for the same list across the scan, where Space marks folders and `d` moves them to the Trash after you confirm, as anywhere in the explorer. Enter jumps to a folder, and `u` or Esc returns to the folders. Space 16X only suggests: it never removes anything without that confirmation, and Move to Trash keeps items until you empty the Trash.

```text
      Size  Suggestion            Path
   5.15 GB  Rust build output     /Users/me/Projects/app/target
   2.10 GB  Xcode build data      /Users/me/Library/Developer/Xcode/DerivedData
  464.7 MB  Node packages         /Users/me/Projects/site/node_modules
   7.71 GB  3 suggestions         [total]
```

Suggestions come from names in the finished scan, so they read nothing new:

| Suggestion | Folder | Recreated by |
| --- | --- | --- |
| Xcode build data | `Library/Developer/Xcode/DerivedData` | Xcode, at the next build |
| Xcode device support | `Library/Developer/Xcode/iOS DeviceSupport` | Xcode, when a device connects |
| Simulator caches | `Library/Developer/CoreSimulator/Caches` | the Simulator |
| App caches | `Library/Caches` | each app, as needed. Quit apps first. |
| Gradle caches | `.gradle/caches` | Gradle, at the next build |
| npm cache | `.npm/_cacache` | npm, at the next install |
| Cache folder | any folder named `.cache` | the programs that use it |
| Node packages | any `node_modules` folder | `npm install` or your package manager |
| Rust build output | `target` beside `Cargo.toml` | `cargo build` |

A folder is listed only when it was read completely and uses at least 1 MB, and folders inside a listed one aren't listed again, so `node_modules` inside `node_modules` counts once. Xcode archives and simulator devices aren't suggested, because they can't be recreated. JSON reports put `cleanupBytes` and `suggestions` in place of `depth` and `entries`. Each suggestion has `path`, `rule` (a stable name, such as `node-modules` or `rust-target`), `label`, `allocatedBytes`, `apparentBytes` and `itemCount`. `--cleanup` takes no `--depth`, `--top`, `--types` or `--by-owner`.

## Scan history

```text
space16x history save [PATH] [--threads N] [EXCLUDES]
space16x history [list] [PATH] [--json]
space16x history diff [PATH] [--since DAYS] [--depth N | --top N] [--apparent] [--json] [--threads N]
```

`history save` scans PATH and keeps the scan, so you can later see what grew since then. `history` lists the kept scans, newest first, of PATH or of every folder. `history diff` compares PATH now with its newest kept scan, using that scan's exclude patterns, and shows the same report as [`diff`](#compare-scans-with-diff). With `--since DAYS`, it uses the newest kept scan that is at least DAYS days old, which answers "what grew this week?":

```sh
space16x history save /srv               # for example, daily from cron
space16x history /srv
space16x history diff /srv --since 7
space16x history diff ~ --top 10
```

```text
Saved                   Size      Items  Path
2026-11-21 06:00   511.42 GB    1102596  /srv
2026-11-14 06:00   498.10 GB    1088120  /srv
```

Only `history save` writes anything. Kept scans are `scan --export` files, so `--import` and `diff` open them too, in `$XDG_STATE_HOME/space16x/history`, or `~/.local/state/space16x/history` when `XDG_STATE_HOME` isn't set. The folder is readable only by you. `index.jsonl` there lists each scan with its totals, one JSON object per line. Each scan holds every path, so a large scan can take tens of megabytes. Space 16X never deletes them: remove old ones from the folder, or move them to the Trash with the explorer, when you no longer need them. `history list --json` writes `schemaVersion`, `edition`, `directory` and `scans`, each with `file`, `path`, `scope`, `savedAt` (Unix seconds), `allocatedBytes`, `apparentBytes`, `itemCount` and `isPartial`. `history save` exits like `scan`, and `history diff` like `diff`. Both exit with `1` when there is no matching kept scan.

## Explain my disk

```text
space16x explain [PATH] [--json]
```

`space16x explain` shows the space on PATH's volume that a scan can't see, which is why a folder's scan rarely matches `df` or Finder. PATH defaults to the current directory. It reads system figures and metadata only, never file contents, and changes nothing.

On macOS, an APFS volume shares its container's space with the container's other volumes, so `explain` lists each mounted volume's own used space: the system volume, Preboot, Recovery, VM (which holds swap files) and your data. It also shows **purgeable** space, which macOS frees when an app needs it, such as caches, iCloud copies of downloaded files and local snapshots, and the Time Machine **local snapshots** that `tmutil listlocalsnapshots` lists. Snapshots keep the space of files deleted since they were taken until macOS removes them.

```text
Volume      /System/Volumes/Data (apfs, /dev/disk3s5)
Size        2.00 TB
Used        847.54 GB, shared by the volumes of its APFS container
Free        1.15 TB
Purgeable   10.39 GB of caches, iCloud copies and snapshots that macOS frees when needed
Snapshots   none

      Used  APFS volume
 813.65 GB  /System/Volumes/Data
  13.76 GB  /
  10.81 GB  /System/Volumes/Preboot
   6.44 GB  /System/Volumes/VM
   2.56 GB  /Volumes/Recovery
   24.3 MB  /System/Volumes/Update
```

On Linux, it shows the blocks reserved for root, which `df` counts as neither used nor free, and **deleted files that programs still hold open**. Their space stays in use until the program closes the file or exits, as for a log file deleted while a service writes to it. Run it as root to include every user's processes. Otherwise standard error says how many processes were hidden.

```text
Volume      / (ext4, /dev/sda1)
Size        100.00 GB
Used        60.00 GB
Free        35.00 GB
Reserved    5.00 GB for root, which df counts as neither used nor free
Open files  1.80 GB in 1 deleted file that programs still hold open

      Size       PID  Program          Path
   1.80 GB      1234  java             /var/log/app.log

Programs free this space when they close these files or exit.
```

JSON reports have `schemaVersion`, `edition`, `volume` (`path`, `device`, `type`, `totalBytes`, `usedBytes`, `availableBytes`), `reservedBytes`, `purgeableBytes` (`null` where the system doesn't report it), `snapshots` (each with `name` and the local `date` of a Time Machine snapshot, or `null` when they couldn't be listed), `containerVolumes` (each with `path` and `usedBytes`), `deletedOpenFiles` (each with `path`, `allocatedBytes`, `apparentBytes`, `pid` and `program`) and `hiddenProcesses`. The lists are empty on the other system. The exit code is `0`, or `1` for errors.
