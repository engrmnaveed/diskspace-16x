# CLI reference

```text
space16x [PATH]
space16x scan [PATH] [--depth N | --top N] [--json]
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
| `e` | Show scan issues. |
| `c` | Cancel an active scan and keep the available preview. |
| ? | Show keyboard help. |
| `q` or Ctrl-C | Exit the explorer. |

## Generate a report

```sh
space16x scan "$HOME/Downloads"
space16x scan "/Volumes/Work" --depth 2
space16x scan --top 10 --json > report.json
```

| Option | Behavior |
| --- | --- |
| `--depth N` | List entries through N levels. The default is one level. Use zero for the root total alone. Directory totals include all counted descendants. |
| `--top N` | List up to N largest files in the scan by allocated size. Use zero for the root total alone. |
| `--json` | Write a machine-readable report on standard output. |

Use either `--depth` or `--top` once per command. Reports use allocated bytes. Entries within a folder and the top files are ranked descending by allocated bytes, with ties ordered by original path bytes.

The scanner reads filesystem metadata, skips symbolic links and mounts on other devices, and counts each hard-linked file once. A later alias can appear as a zero-byte entry. In the top-files report only the counted path appears. Cloud-only entries detected on macOS contribute zero bytes, and their directory contents are left unexpanded.

## Scan exit codes

| Code | Meaning |
| --- | --- |
| `0` | Complete scan. |
| `1` | Fatal scan, output, or usage error. See standard error for details. |
| `2` | Partial scan. Totals include the data that could be counted. See the issue paths in JSON output or standard error. |

Expected exclusions such as symlinks and different-device mounts are part of a complete scan. Read errors and skipped provider metadata due to a timeout make a scan partial.

## JSON schema 2

| Field | Meaning |
| --- | --- |
| `schemaVersion` | The integer `2`. |
| `scope` | Absolute path to the scan root. |
| `allocatedBytes` | Counted regular-file allocation in bytes. |
| `isPartial` | Boolean. If true, errors qualify the totals. |
| `issues` | Array of objects with a `path` and human-readable `message`. |
| `depth`, `entries` | Depth report only. The selected depth and a flattened list of entries with `path`, `allocatedBytes`, and `kind` (`directory` or `file`). |
| `topLimit`, `topFiles` | Top report only. The requested limit and a ranked list of files with `path` and `allocatedBytes`. |

If a path contains bytes that cannot be decoded as UTF-8, its display value substitutes invalid sequences. The optional `scopeBytes` or `pathBytes` field contains the original path encoded as lowercase hexadecimal. Size values are unsigned integers. Review any report for personal paths before sharing.

[View an example top-files report](example-report.json).
