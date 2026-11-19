#!/usr/bin/env python3
"""Exercise the CLI as an external consumer on macOS and Linux."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile


def run(binary, *arguments, status=0, cwd=None):
    result = subprocess.run(
        [binary, *map(str, arguments)], capture_output=True, text=True, check=False, cwd=cwd
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
    top = json.loads(run(binary, "scan", root, "--json", "--top", 2).stdout)
    assert top["schemaVersion"] == 2
    assert top["scope"] == str(root)
    assert top["topLimit"] == 2
    assert top["allocatedBytes"] == total_bytes
    assert top["isPartial"] is False and top["issues"] == []
    assert [entry["path"] for entry in top["topFiles"]] == [
        str(path) for path in sorted((first, second), key=lambda path: (-allocated_bytes(path), os.fsencode(path)))
    ]
    assert "logicalBytes" not in top

    depth = json.loads(run(binary, "scan", "--json", cwd=root).stdout)
    assert depth["schemaVersion"] == 2 and depth["scope"] == str(root)
    assert depth["depth"] == 1
    assert depth["entries"] == [{"path": str(data), "allocatedBytes": total_bytes, "kind": "directory"}]

    deeper = json.loads(run(binary, "scan", "--depth", 2, "--json", "--", root).stdout)
    rows = {row["path"]: row for row in deeper["entries"]}
    assert rows[str(first)]["allocatedBytes"] == allocated_bytes(first)
    assert rows[str(alias)]["allocatedBytes"] == 0
    assert rows[str(sparse)]["allocatedBytes"] == allocated_bytes(sparse)

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
        unreadable = run(binary, "scan", blocked, "--json", status=1)
        assert not unreadable.stdout and unreadable.stderr
    finally:
        blocked.chmod(0o700)


def main():
    if len(sys.argv) != 2:
        raise SystemExit(f"Usage: {sys.argv[0]} PATH-TO-SPACE16X")
    binary = str(Path(sys.argv[1]).resolve(strict=True))
    with tempfile.TemporaryDirectory(prefix="space16x-cli-") as directory:
        check(binary, Path(directory).resolve())
    print("CLI acceptance checks passed.")


if __name__ == "__main__":
    main()
