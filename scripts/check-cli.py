#!/usr/bin/env python3
"""Exercise the CLI as an external consumer on macOS and Linux."""

import json
import os
from pathlib import Path
import resource
import subprocess
import sys
import tempfile


def run(binary, *arguments, status=0, cwd=None, preexec_fn=None):
    result = subprocess.run(
        [binary, *map(str, arguments)], capture_output=True, text=True, check=False,
        cwd=cwd, preexec_fn=preexec_fn,
    )
    assert result.returncode == status, (
        f"Expected exit {status} for {arguments}, got {result.returncode}: "
        f"{result.stderr}{result.stdout}"
    )
    return result


def allocated_bytes(path):
    return 512 * path.stat().st_blocks


def check(binary, root):
    version = Path(__file__).with_name("release-version").read_text(encoding="utf-8").strip()
    assert run(binary, "--version").stdout.strip() == f"space16x {version}"

    data = root / "Nested folder"
    data.mkdir()
    first = data / "alpha.dat"
    second = data / "beta.dat"
    for path in (first, second):
        path.write_bytes(os.urandom(8192))
    alias = data / "copy.dat"
    os.link(first, alias)
    sparse = data / "sparse.dat"
    with sparse.open("wb") as stream:
        stream.truncate(32 * 1024 * 1024)
    symlink = root / "symlink"
    symlink.symlink_to(data, target_is_directory=True)

    text = run(binary, cwd=root).stdout
    assert str(data) in text and str(first) not in text, text
    assert str(first) in run(binary, "scan", root, "--top", 2).stdout

    total_bytes = sum(map(allocated_bytes, (first, second, sparse)))
    apparent_total = sum(path.stat().st_size for path in (first, second, sparse))
    top = json.loads(run(binary, "scan", root, "--json", "--top", 2).stdout)
    assert top["schemaVersion"] == 3
    assert top["edition"] == "basic" and top["rankedBy"] == "allocated"
    assert top["scope"] == str(root)
    assert top["topLimit"] == 2
    assert top["allocatedBytes"] == total_bytes
    assert top["apparentBytes"] == apparent_total
    # The folder and three counted files. The second hard link and the symlink aren't items.
    assert top["itemCount"] == 4
    assert top["isPartial"] is False and top["issues"] == []
    assert [entry["path"] for entry in top["topFiles"]] == [
        str(path) for path in sorted((first, second), key=lambda path: (-allocated_bytes(path), os.fsencode(path)))
    ]
    assert "logicalBytes" not in top

    depth = json.loads(run(binary, "scan", "--json", cwd=root).stdout)
    assert depth["schemaVersion"] == 3 and depth["scope"] == str(root)
    assert depth["depth"] == 1
    assert depth["entries"] == [{
        "path": str(data), "allocatedBytes": total_bytes, "apparentBytes": apparent_total,
        "kind": "directory", "itemCount": 3,
    }]

    # --apparent ranks by file length, so the sparse file leads despite its small allocation.
    apparent = json.loads(run(binary, "scan", root, "--top", 2, "--apparent", "--json").stdout)
    assert apparent["rankedBy"] == "apparent"
    assert [entry["path"] for entry in apparent["topFiles"]] == [str(sparse), str(first)]
    assert apparent["topFiles"][0]["apparentBytes"] == sparse.stat().st_size
    plain_apparent = run(binary, "scan", root, "--top", 1, "--apparent").stdout
    assert plain_apparent.splitlines()[0].endswith(str(sparse)), plain_apparent

    deeper = json.loads(run(binary, "scan", "--depth", 2, "--json", "--", root).stdout)
    rows = {row["path"]: row for row in deeper["entries"]}
    assert rows[str(first)]["allocatedBytes"] == allocated_bytes(first)
    assert rows[str(alias)]["allocatedBytes"] == 0 and rows[str(alias)]["apparentBytes"] == 0
    assert "itemCount" not in rows[str(first)]
    assert rows[str(sparse)]["allocatedBytes"] == allocated_bytes(sparse)

    # Every thread count gives the same report, hard links included, and --threads
    # takes 1 to 64.
    single = run(binary, "scan", root, "--depth", 3, "--json", "--threads", 1).stdout
    for threads in (2, 8, 64):
        parallel = run(binary, "scan", root, "--depth", 3, "--json", "--threads", threads).stdout
        assert parallel == single, threads
    for invalid in ("0", "65", "two"):
        assert run(binary, "scan", root, "--threads", invalid, status=1).stderr

    # Excludes skip matches without reading them and keep the scan complete.
    patterns = root.parent / f"{root.name}-excludes.txt"
    patterns.write_text("# comment\n\nbeta.dat\n", encoding="utf-8")
    try:
        excluded = json.loads(run(
            binary, "scan", root, "--json", "--exclude", "*.dat", "--exclude-from", patterns,
        ).stdout)
        assert excluded["excludes"] == ["*.dat", "beta.dat"], excluded["excludes"]
        assert excluded["excludedCount"] == 4 and excluded["allocatedBytes"] == 0
        assert excluded["itemCount"] == 1 and excluded["isPartial"] is False
        folder_only = json.loads(run(binary, "scan", root, "--json", "--exclude", "/Nested folder/").stdout)
        assert folder_only["excludedCount"] == 1 and folder_only["entries"] == []
        summary = run(binary, "scan", root, "--exclude", "sparse.dat")
        assert "excluded 1 path matching 1 pattern" in summary.stderr, summary.stderr
        assert str(sparse) not in summary.stdout
        assert run(binary, "scan", root, "--exclude-from", root / "missing.txt", status=1).stderr
        assert run(binary, "scan", root, "--exclude", "/", status=1).stderr
    finally:
        patterns.unlink()
    unfiltered = json.loads(run(binary, "scan", root, "--json").stdout)
    assert unfiltered["excludedCount"] == 0 and unfiltered["excludes"] == []

    zero_depth = json.loads(run(binary, "scan", root, "--depth", 0, "--json").stdout)
    assert zero_depth["entries"] == [] and zero_depth["allocatedBytes"] == total_bytes
    zero_top = json.loads(run(binary, "scan", root, "--top", 0, "--json").stdout)
    assert zero_top["topFiles"] == [] and zero_top["topLimit"] == 0

    for path in (root / "missing", first, symlink):
        error = run(binary, "scan", path, "--json", status=1)
        assert not error.stdout and error.stderr
    assert run(binary, "scan", root, "--top", -1, status=1).stderr
    assert run(binary, "scan", root, "--top", 2, "--depth", 1, status=1).stderr

    if os.geteuid() == 0:
        print("Permission checks skipped because the fixture runs as root.")
        return

    blocked = root / "blocked"
    blocked.mkdir()
    (blocked / "hidden").write_text("excluded by permissions", encoding="utf-8")
    blocked.chmod(0)
    try:
        partial = json.loads(run(binary, "scan", root, "--json", status=2).stdout)
        assert partial["isPartial"] is True
        assert partial["issues"] and partial["issues"][0]["path"] == str(blocked)
        assert partial["allocatedBytes"] == total_bytes
        # Plain output summarizes protected paths instead of listing each one.
        plain = run(binary, "scan", root, status=2)
        assert "[protected]" in plain.stdout and "[partial total]" in plain.stdout
        assert "skipped 1 protected path (permission denied" in plain.stderr
        assert f"{blocked}:" not in plain.stderr
        unreadable = run(binary, "scan", blocked, "--json", status=1)
        assert not unreadable.stdout and unreadable.stderr
    finally:
        blocked.chmod(0o700)


def check_deep_tree(binary, root):
    # Each traversal level keeps a descriptor open. Start only the child at launchd's soft limit.
    deepest = root.joinpath(*(["d"] * 300))
    deepest.mkdir(parents=True)
    leaf = deepest / "leaf.dat"
    leaf.write_bytes(os.urandom(4096))

    def set_launchd_limit():
        _, hard_limit = resource.getrlimit(resource.RLIMIT_NOFILE)
        resource.setrlimit(resource.RLIMIT_NOFILE, (256, hard_limit))

    scan = run(binary, "scan", root, "--json", preexec_fn=set_launchd_limit)
    report = json.loads(scan.stdout)
    assert report["isPartial"] is False and report["issues"] == []
    assert report["allocatedBytes"] == allocated_bytes(leaf)


def main():
    if len(sys.argv) != 2:
        raise SystemExit(f"Usage: {sys.argv[0]} PATH-TO-SPACE16X")
    binary = str(Path(sys.argv[1]).resolve(strict=True))
    with tempfile.TemporaryDirectory(prefix="space16x-cli-") as directory:
        check(binary, Path(directory).resolve())
    with tempfile.TemporaryDirectory(prefix="space16x-deep-") as directory:
        check_deep_tree(binary, Path(directory).resolve())
    print("CLI acceptance checks passed.")


if __name__ == "__main__":
    main()
