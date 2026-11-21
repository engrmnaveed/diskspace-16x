#!/usr/bin/env python3
"""Exercise the CLI as an external consumer on macOS and Linux."""

import json
import os
from pathlib import Path
import resource
import shutil
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


def edition(binary):
    """`pro` or `basic`, from the version line, which must match scripts/release-version."""
    version = Path(__file__).with_name("release-version").read_text(encoding="utf-8").strip()
    reported = run(binary, "--version").stdout.strip()
    assert reported in (f"space16x {version}", f"space16x {version} (pro)"), reported
    return "pro" if reported.endswith(" (pro)") else "basic"


def check(binary, root):
    if edition(binary) == "basic":
        # Basic answers Pro subcommands with an install hint.
        hint = run(binary, "diff", "old.json", "new.json", status=1)
        assert "Space 16X Pro" in hint.stderr and "--pro" in hint.stderr and not hint.stdout
        assert "install.sh --pro" in run(binary, "--help").stdout

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
    assert top["edition"] == edition(binary) and top["rankedBy"] == "allocated"
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

    check_export_and_import(binary, root, first, alias, sparse)
    check_ncdu_and_gdu(binary, root)

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
        # A partial scan exports its issues, and the import is partial too.
        saved = run(binary, "scan", root, "--export", "-", status=2).stdout
        issues = json.loads(saved)[2]["space16x_issues"]
        assert [issue["path"] for issue in issues] == [str(blocked)], issues
        assert issues[0]["kind"] == "permissionDenied"
        imported = import_text(binary, saved, status=2)
        assert imported.stdout == plain.stdout, imported.stdout
        assert "skipped 1 protected path" in imported.stderr, imported.stderr
    finally:
        blocked.chmod(0o700)


def import_text(binary, text, status=0):
    """Runs --import - with `text` on standard input."""
    result = subprocess.run(
        [binary, "--import", "-"], input=text, capture_output=True, text=True, check=False,
    )
    assert result.returncode == status, f"{result.returncode}: {result.stderr}"
    return result


def entries(directory):
    """The info objects of an ncdu directory array, by name, with folders as lists."""
    return {
        (item[0] if isinstance(item, list) else item)["name"]: item for item in directory[1:]
    }


def check_export_and_import(binary, root, first, alias, sparse):
    """Exports are ncdu JSON that --import, ncdu and gdu read."""
    export = root.parent / f"{root.name}-export.json"
    try:
        listing = run(binary, "scan", root, "--export", export)
        assert listing.stdout == run(binary, "scan", root).stdout
        saved = json.loads(export.read_text(encoding="utf-8"))
        assert saved[:2] == [1, 2], saved[:2]
        assert saved[2]["progname"] == "space16x" and saved[2]["space16x_issues"] == []
        assert saved[3][0] == {"name": str(root)}
        folder = entries(saved[3])["Nested folder"]
        files = entries(folder)
        assert files[first.name] == {
            "name": first.name, "asize": 8192, "dsize": allocated_bytes(first),
        }
        assert files[alias.name] == {
            "name": alias.name, "asize": 0, "dsize": 0, "space16x_state": "hardLinkAlias",
        }
        assert files[sparse.name]["asize"] == sparse.stat().st_size
        assert "symlink" not in entries(saved[3])

        streamed = run(binary, "scan", root, "--export", "-").stdout
        assert json.loads(streamed)[3] == saved[3]
        assert run(binary, "--import", export).stdout == listing.stdout
        assert import_text(binary, streamed).stdout == listing.stdout
        excluded = run(binary, "scan", root, "--exclude", "*.dat", "--export", "-").stdout
        assert json.loads(excluded)[2]["space16x_excludes"] == ["*.dat"]
        assert "excluded 4 paths matching 1 pattern" in import_text(binary, excluded).stderr

        for arguments in (
            ("scan", root, "--export", "-", "--json"),
            ("scan", root, "--export", root / "missing" / "scan.json"),
            ("--import", root / "missing.json"),
            ("--import", first),
            ("--import", export, root),
        ):
            assert run(binary, *arguments, status=1).stderr
    finally:
        export.unlink(missing_ok=True)

    # Exports from ncdu and gdu use their own fields, such as folder sizes and inodes.
    foreign = json.dumps([1, 2, {"progname": "ncdu"}, [
        {"name": "/srv", "asize": 4096, "dsize": 4096, "dev": 9},
        {"name": "one", "asize": 10, "dsize": 4096, "ino": 5, "nlink": 2},
        [{"name": "logs", "dsize": 4096}, {"name": "two", "asize": 10, "dsize": 4096, "ino": 5, "hlnkc": True}],
        {"name": "proc", "excluded": "otherfs"},
    ]])
    lines = import_text(binary, foreign).stdout.splitlines()
    assert lines == ["      4 KB  /srv/one", "       0 B  /srv/logs", "      4 KB  /srv  [total]"], lines


def check_ncdu_and_gdu(binary, root):
    """When ncdu or gdu is installed, each reads our export and we read theirs."""
    expected = run(binary, "scan", root).stdout
    export = root.parent / f"{root.name}-ours.json"
    theirs = root.parent / f"{root.name}-theirs.json"
    run(binary, "scan", root, "--export", export)
    try:
        for tool, write, read in (
            ("ncdu", ["-0", "-x", "-o", theirs, root], ["-0", "-f", export, "-o", "-"]),
            ("gdu", ["-n", "-o", theirs, root], ["-n", "-f", export]),
        ):
            path = shutil.which(tool)
            if path is None:
                print(f"{tool} isn't installed, so its interoperability check was skipped.")
                continue
            run(path, *write)
            imported = run(binary, "--import", theirs).stdout
            assert imported == expected, f"{tool}:\n{imported}\n{expected}\n{theirs.read_text()}"
            output = run(path, *read).stdout
            assert "Nested folder" in output, output
            if tool == "ncdu":
                reexport = json.loads(output)
                assert entries(reexport[3]).keys() == {"Nested folder"}, reexport
            print(f"{tool} opened the export, and Space 16X opened {tool}'s.")
    finally:
        export.unlink(missing_ok=True)
        theirs.unlink(missing_ok=True)


def check_volumes(binary):
    """`volumes` lists the root filesystem in text and JSON."""
    text = run(binary, "volumes").stdout.splitlines()
    assert text[0].split() == ["Size", "Used", "Free", "Use%", "Inodes", "Type", "Mounted", "on"], text
    assert any(line.endswith(" /") for line in text[1:]), text
    report = json.loads(run(binary, "volumes", "--json").stdout)
    assert report["schemaVersion"] == 3 and report["edition"] == edition(binary), report
    root = next(volume for volume in report["volumes"] if volume["path"] == "/")
    assert root["totalBytes"] > 0 and root["usedBytes"] <= root["totalBytes"], root
    assert root["availableBytes"] <= root["totalBytes"] and root["type"], root
    everything = json.loads(run(binary, "volumes", "--all", "--json").stdout)
    assert len(everything["volumes"]) >= len(report["volumes"])
    for invalid in (("volumes", "/"), ("volumes", "--json", "--json")):
        assert run(binary, *invalid, status=1).stderr


def check_diff(binary):
    """Pro: diff compares a saved scan with a folder or another saved scan."""
    with tempfile.TemporaryDirectory(prefix="space16x-diff-") as directory:
        base = Path(directory).resolve()
        root = base / "data"
        (root / "grow").mkdir(parents=True)
        (root / "gone").mkdir()
        (root / "grow" / "a.bin").write_bytes(os.urandom(10_000))
        (root / "gone" / "b.bin").write_bytes(os.urandom(20_000))
        old = base / "old.json"
        run(binary, "scan", root, "--exclude", "*.log", "--export", old)
        (root / "grow" / "a.bin").write_bytes(os.urandom(500_000))
        shutil.rmtree(root / "gone")
        (root / "new.bin").write_bytes(os.urandom(30_000))
        # The saved scan's excludes apply to the new scan, so the log isn't a change.
        (root / "big.log").write_bytes(os.urandom(900_000))

        report = json.loads(run(binary, "diff", old, root, "--json").stdout)
        assert report["edition"] == "pro" and report["depth"] == 1, report
        assert [Path(row["path"]).name for row in report["entries"]] == ["grow", "new.bin", "gone"], report
        rows = {Path(row["path"]).name: row for row in report["entries"]}
        assert rows["grow"]["changeBytes"] == allocated_bytes(root / "grow" / "a.bin") - rows["grow"]["oldBytes"]
        assert rows["new.bin"]["oldBytes"] is None and rows["gone"]["newBytes"] is None
        assert report["changeBytes"] == report["new"]["allocatedBytes"] - report["old"]["allocatedBytes"]
        text = run(binary, "diff", old, root).stdout
        assert text.splitlines()[0].split() == ["Change", "Old", "New", "Path"], text
        assert "[added]" in text and "[removed]" in text and text.rstrip().endswith("[total]"), text

        top = json.loads(run(binary, "diff", old, root, "--top", 2, "--json").stdout)
        assert [Path(row["path"]).name for row in top["topFiles"]] == ["a.bin", "new.bin"], top
        newer = base / "new.json"
        run(binary, "scan", root, "--exclude", "*.log", "--export", newer)
        piped = subprocess.run(
            [binary, "diff", "-", newer, "--json"], input=old.read_text(encoding="utf-8"),
            capture_output=True, text=True, check=False,
        )
        assert piped.returncode == 0 and json.loads(piped.stdout)["changeBytes"] == report["changeBytes"]
        same = json.loads(run(binary, "diff", newer, root, "--json").stdout)
        assert same["changeBytes"] == 0 and same["entries"] == [], same
        for invalid in (("diff", old), ("diff", "-", "-"), ("diff", base / "missing.json", root)):
            assert run(binary, *invalid, status=1).stderr


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
    check_volumes(binary)
    if edition(binary) == "pro":
        check_diff(binary)
    print("CLI acceptance checks passed.")


if __name__ == "__main__":
    main()
