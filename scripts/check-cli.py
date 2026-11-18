#!/usr/bin/env python3
"""Exercise the CLI as an external consumer on macOS and Linux."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile


def run(binary, *arguments, status=0):
    result = subprocess.run(
        [binary, *map(str, arguments)], capture_output=True, text=True, check=False
    )
    assert result.returncode == status, (
        f"Expected exit {status} for {arguments}, got {result.returncode}: "
        f"{result.stderr}{result.stdout}"
    )
    return result


def check(binary, root):
    version = Path(__file__).with_name("release-version").read_text(encoding="utf-8").strip()
    assert run(binary, "--version").stdout.strip() == f"space16x {version}"

    data = root / "Nested folder"
    data.mkdir()
    first = data / "alpha.dat"
    second = data / "beta.dat"
    for path in (first, second):
        path.write_bytes(os.urandom(8192))
    os.link(first, data / "copy.dat")

    sparse = data / "sparse.dat"
    with sparse.open("wb") as stream:
        stream.truncate(32 * 1024 * 1024)
    (root / "symlink").symlink_to(data, target_is_directory=True)

    text = run(binary, "scan", root, "--top", 2).stdout
    assert str(first) in text, text

    allocated = json.loads(run(binary, "scan", root, "--json", "--top", 2).stdout)
    assert allocated["schemaVersion"] == 1
    assert allocated["scope"] == str(root)
    assert allocated["metric"] == "allocated"
    assert allocated["topLimit"] == 2
    assert allocated["logicalBytes"] == first.stat().st_size + second.stat().st_size + sparse.stat().st_size
    assert allocated["allocatedBytes"] == 512 * sum(
        path.stat().st_blocks for path in (first, second, sparse)
    )
    assert allocated["isPartial"] is False
    assert allocated["issues"] == []
    assert [entry["path"] for entry in allocated["topFiles"]] == [str(first), str(second)]

    logical = json.loads(
        run(binary, "scan", "--metric", "logical", "--json", "--", root).stdout
    )
    assert logical["metric"] == "logical"
    assert logical["topLimit"] == 20
    assert logical["topFiles"][0]["path"] == str(sparse)
    assert logical["topFiles"][0]["logicalBytes"] == sparse.stat().st_size

    zero = json.loads(run(binary, "scan", root, "--top", 0, "--json").stdout)
    assert zero["topFiles"] == [] and zero["topLimit"] == 0

    for path in (root / "missing", first, root / "symlink"):
        error = run(binary, "scan", path, "--json", status=1)
        assert not error.stdout and error.stderr
    assert run(binary, "scan", root, "--top", -1, status=1).stderr

    if os.geteuid() == 0:
        print("Permission checks skipped because the fixture runs as root.")
        return

    blocked = root / "blocked"
    blocked.mkdir()
    (blocked / "hidden").write_text("not included", encoding="utf-8")
    blocked.chmod(0)
    try:
        partial = json.loads(run(binary, "scan", root, "--json", status=2).stdout)
        assert partial["isPartial"] is True
        assert partial["issues"] and partial["issues"][0]["path"] == str(blocked)
        assert partial["logicalBytes"] == allocated["logicalBytes"]
        assert partial["allocatedBytes"] == allocated["allocatedBytes"]
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
