# DiskUse16x CLI

A read-only disk space explorer for the terminal, on macOS and Linux. It scans a folder or disk, ranks what uses the space by allocated size, and opens an interactive explorer, or writes text and JSON reports for scripts. Scans read file metadata only, and the one change it makes to your files is a Move to Trash you confirm.

```sh
cargo install diskuse16x                  # Basic
cargo install diskuse16x --features pro   # Pro: diff, history, duplicates, cleanup suggestions and more
diskuse16x ~
```

Releases also publish checksummed archives, a one-line installer, Homebrew formulas and Linux packages, all of which add `du16x` as a short name. After `cargo install`, add it yourself with `ln -s diskuse16x "$HOME/.cargo/bin/du16x"`.

- [Installation guide](https://github.com/engrmnaveed/diskuse16x/blob/main/docs/install.md)
- [CLI reference](https://github.com/engrmnaveed/diskuse16x/blob/main/docs/cli.md) and [Pro reference](https://github.com/engrmnaveed/diskuse16x/blob/main/docs/cli-pro.md)
- [User guide](https://github.com/engrmnaveed/diskuse16x/blob/main/docs/guide/README.md), and the Mac app in the [repository](https://github.com/engrmnaveed/diskuse16x)

MIT licensed. DiskUse16x was called Space 16X before 1.0.
