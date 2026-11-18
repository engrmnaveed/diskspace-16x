# CLI reference

```text
space16x scan PATH [--top N] [--metric allocated|logical] [--json]
space16x --version
space16x --help
```

The root must be a readable directory. Put a shell-quoted directory path after `scan`, or use `--` before a path that starts with a dash.

| Option | Behavior |
| --- | --- |
| `--top N` | Show up to N ranked files. The default is 20. Use 0 for totals only. |
| `--metric allocated` | Rank by allocated bytes. This is the default. |
| `--metric logical` | Rank by logical bytes. |
| `--json` | Write a machine-readable report to standard output. |

Ties use the bytewise UTF-8 order of full paths. A hard-linked file contributes its bytes at the first path found during a depth-first traversal with names ordered bytewise. Later aliases are visible when browsing their folders in the app.

## Exit codes

| Code | Meaning |
| --- | --- |
| `0` | Complete scan. |
| `1` | Fatal scan, output, or usage error. Read standard error for details. |
| `2` | Partial results. The output includes valid totals for the data that was counted and details of skipped paths. |

Expected scope exclusions such as symlinks and mounts on other devices are part of a complete scan. Unreadable paths and other scan errors mark results partial. Paths with names that cannot be represented as UTF-8 are skipped and use percent-encoded name bytes in the issue.

On macOS, cloud-only references contribute zero bytes and are excluded from the largest-files list. Cloud-only directories remain unexpanded. Metadata calls in known provider locations have a time limit. A timeout lists the affected path in `issues` and makes the scan partial.

## JSON schema 1

| Field | Meaning |
| --- | --- |
| `schemaVersion` | The integer `1` for this report format. |
| `scope` | Absolute path to the scan root. |
| `metric` | Ranking metric, `allocated` or `logical`. |
| `topLimit` | Requested maximum number of ranked files. |
| `allocatedBytes`, `logicalBytes` | The root's counted regular-file totals in bytes. |
| `isPartial` | Boolean. If true, errors qualify the reported totals. |
| `issues` | Array of objects with a `path` and human-readable `message`. |
| `topFiles` | Ranked array of objects with `path`, `allocatedBytes`, and `logicalBytes`. |

Sizes are unsigned byte counts encoded as JSON integers. Shell tools and clients should preserve integer precision when working with very large totals. Reports can contain personal paths. Review them before sharing.

[View an example report](example-report.json).
