# CLI reference

```text
space16x [--no-color] [--read-only] [EXCLUDES] [PATH]
space16x scan [PATH] [--depth N | --top N] [--apparent] [--json] [EXCLUDES]
space16x --version
space16x --help
```

**PATH** defaults to the current directory. The scan root must be a readable directory. Quote paths containing spaces. Use `--` to pass a path that begins with a dash.

## Explore a folder

Run `space16x` or `space16x PATH` from an interactive terminal. Scan progress and the first levels of results appear as the scanner works. If either standard input or standard output is redirected, or `TERM=dumb` is set, the same command prints a one-level plain-text report.

| Key | Action |
| --- | --- |
| ↑ / ↓ or `k` / `j` | Move the selection. |
| Enter, →, or `l` | Open a folder, inspect a file, or jump from a largest-file result to its containing folder. |
| ←, Backspace, Esc, or `h` | Go back. |
| / | Filter the current view by path. Press Enter to keep the filter or Esc to undo the edit. |
| `f` | Switch between browsing and the largest-files view. |
| `s` | Sort folder contents by size, item count, or name. The sorted column is marked in the heading. |
| `a` | Switch between allocated and apparent sizes, in both views. |
| Space | Mark or unmark the selected item for the Trash, then move down. |
| `d` or Delete | Move the marked items, or the selected item if none are marked, to the Trash after you confirm with `y`. |
| `e` | Show scan issues. |
| `c` | Cancel an active scan and keep the available preview. |
| ? | Show keyboard help. |
| `q` or Ctrl-C | Exit the explorer. |

Wide terminals show an **Items** column with the number of files and folders inside each folder, which helps find folders that use up inodes. File details show both the allocated and the apparent size. Set `NO_COLOR` to any value, or pass `--no-color`, to draw the explorer without colors. The selection then uses reverse video.

## Move items to the Trash

Moving items to the Trash is the only change Space 16X makes to your files. Press Space to mark items in any folder or in the largest-files view. The heading shows how many are marked and their allocated size. Press `d`, and the footer asks you to confirm the item count and size. Press `y` to move them, or any other key to keep them. With nothing marked, `d` offers the selected item.

Moved items leave the results at once. Folder totals, rankings, and item counts update without a rescan. **Space is freed when you empty the Trash.** Moving a hard-linked file, an APFS clone, or a file that a snapshot keeps may free nothing, because another path or the snapshot still uses its blocks.

- On macOS, items go to the Trash on their volume, through the same macOS API that Finder uses for Move to Trash.
- On Linux, items go to the [FreeDesktop.org Trash](https://specifications.freedesktop.org/trash-spec/latest/) that desktop file managers use. Items on your home disk go to `~/.local/share/Trash`, or `$XDG_DATA_HOME/Trash`. Items on other disks go to `.Trash/UID` or `.Trash-UID` at the top of that disk. Restore them from a file manager's Trash or with a tool such as `gio trash --restore`.
- Items are renamed within their own disk and never copied. If a disk has no Trash you can use, such as a read-only mount, the item stays where it is and the footer says why.

The explorer doesn't move items while a scan is running, or items it couldn't fully read: cloud-only, protected, unscanned, or incomplete ones. Pass `--read-only`, or set `SPACE16X_READ_ONLY=1`, to turn marking and trashing off, for example on a shared server.

## Exclude files and folders

Both the explorer and `scan` accept exclude patterns. A matching file or folder is skipped without reading its metadata or contents, so excluding a large tree such as `node_modules` also makes the scan faster.

```sh
space16x --exclude node_modules --exclude '*.log' ~/Projects
space16x scan / --exclude /proc --exclude /sys --exclude-from ~/.config/space16x-excludes
```

| Option | Behavior |
| --- | --- |
| `--exclude PATTERN` | Skip entries that match the pattern. Repeat it to add patterns. |
| `--exclude-from FILE` | Read one pattern per line. Blank lines and lines starting with `#` are ignored. |

Patterns are a small subset of `.gitignore` syntax, matched against the path relative to the scan root:

| Pattern | Matches |
| --- | --- |
| `node_modules`, `*.log`, `?.txt` | A name at any depth. `*` matches any run of characters within a name, and `?` one character. |
| `build/cache` | Those consecutive folders at any depth. |
| `/build` | `build` directly under the scan root only. |
| `/src/**/target` | `target` anywhere below `src`. A `**` segment matches zero or more folders. |
| `cache/` | Folders named `cache`, but not files. |

Quote patterns so the shell doesn't expand them. Excluded entries are expected exclusions: they don't make a scan partial, and their bytes and items aren't counted. Text reports summarize them on standard error, the explorer's header shows how many were excluded, and JSON lists the patterns and the count.

## Generate a report

```sh
space16x scan "$HOME/Downloads"
space16x scan "/Volumes/Work" --depth 2
space16x scan --top 10 --json > report.json
space16x scan /var/log --top 5 --apparent
```

| Option | Behavior |
| --- | --- |
| `--depth N` | List entries through N levels. The default is one level. Use zero for the root total alone. Directory totals include all counted descendants. |
| `--top N` | List up to N largest files in the scan by allocated size. Use zero for the root total alone. |
| `--apparent` | Show and rank apparent sizes, the file lengths that `ls -l` reports, instead of allocated bytes. Sparse, compressed, and small files can differ a lot between the two. |
| `--json` | Write a machine-readable report on standard output. |

Use either `--depth` or `--top` once per command. Reports use allocated bytes unless `--apparent` is given. Text reports and the explorer show sizes in decimal units, matching Finder and the Mac app: 1 KB is 1,000 bytes and 1 GB is 1,000,000,000 bytes. The explorer's file details add the binary size, such as GiB, and the exact byte count. JSON reports exact byte counts. Entries within a folder and the top files are ranked descending by the reported size, with ties ordered by original path bytes. Apparent sizes follow the same counting rules as allocated bytes: each hard-linked file counts once, and cloud-only files count as zero.

The scanner reads filesystem metadata, skips symbolic links and mounts on other devices, and counts each hard-linked file once. A later alias can appear as a zero-byte entry. In the top-files report only the counted path appears. Cloud-only entries detected on macOS contribute zero bytes, and their directory contents are left unexpanded.

On macOS, a startup-disk scan counts firmlinked directories such as `/Users` at their visible paths. It skips the corresponding directories under `/System/Volumes/Data` when their visible paths are in scope. Data-only directories remain in the report, and a direct scan of `/System/Volumes/Data` includes the firmlinked directories there.

## Scan exit codes

| Code | Meaning |
| --- | --- |
| `0` | Complete scan. |
| `1` | Fatal scan, output, or usage error. See standard error for details. |
| `2` | Partial scan. Totals include the data that could be counted. See the issue paths in JSON output or standard error. |

Expected exclusions such as symlinks and different-device mounts are part of a complete scan. Read errors and skipped provider metadata due to a timeout make a scan partial.

Paths the operating system refuses to open are protected paths. These include folders readable only by root, and on macOS, folders guarded by privacy controls or System Integrity Protection. They also make a scan partial with exit code `2`, because their contents aren't counted, but they are reported calmly. Standard error lists each other issue and summarizes protected paths in one line, and JSON output lists every path in `issues`. Text reports mark them `[protected]`, and the explorer shows a protected state instead of a warning. On macOS, granting your terminal app Full Disk Access lets the CLI read privacy-protected folders. See [Full Disk Access](install.md#full-disk-access).

## JSON schema 3

| Field | Meaning |
| --- | --- |
| `schemaVersion` | The integer `3`. |
| `edition` | `basic` for this build. |
| `scope` | Absolute path to the scan root. |
| `rankedBy` | `allocated`, or `apparent` with `--apparent`. Entries and top files are ordered by this size. |
| `allocatedBytes` | Counted regular-file allocation in bytes. |
| `apparentBytes` | Counted regular-file lengths in bytes. |
| `itemCount` | Files and folders below the scan root, counting each hard-linked file once. Symbolic links, special files, other-device mounts, and excluded entries aren't items. |
| `excludedCount` | Entries skipped by exclude patterns. Their contents aren't visited, so they count once each. |
| `excludes` | The exclude patterns in effect, as given. |
| `isPartial` | Boolean. If true, skipped paths qualify the totals: errors, protected paths, or both. |
| `issues` | Array of objects with a `path` and human-readable `message`. |
| `depth`, `entries` | Depth report only. The selected depth and a flattened list of entries with `path`, `allocatedBytes`, `apparentBytes`, `kind` (`directory` or `file`), and for directories `itemCount`. |
| `topLimit`, `topFiles` | Top report only. The requested limit and a ranked list of files with `path`, `allocatedBytes`, and `apparentBytes`. |

Schema 3 keeps every schema 2 field with the same meaning and adds `edition`, `rankedBy`, `apparentBytes`, `itemCount`, `excludedCount`, and `excludes`. A consumer that ignores unknown fields reads both. Check `schemaVersion` only if you need the new fields.

If a path contains bytes that cannot be decoded as UTF-8, its display value substitutes invalid sequences. The optional `scopeBytes` or `pathBytes` field contains the original path encoded as lowercase hexadecimal. Size values are unsigned integers. Review any report for personal paths before sharing.

[View an example top-files report](example-report.json).
