#!/usr/bin/env python3
"""Exercise the CLI as an external consumer on macOS and Linux."""

import json
import os
import pwd
from pathlib import Path
import resource
import shutil
import subprocess
import sys
import tempfile
import time


def run(binary, *arguments, status=0, cwd=None, preexec_fn=None, env=None):
    result = subprocess.run(
        [binary, *map(str, arguments)], capture_output=True, text=True, check=False,
        cwd=cwd, preexec_fn=preexec_fn, env=env,
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
        for command in (
            ("diff", "old.json", "new.json"), ("history",), ("history", "save", root), ("explain",),
        ):
            hint = run(binary, *command, status=1)
            assert "Space 16X Pro" in hint.stderr and "--pro" in hint.stderr and not hint.stdout
        for option in (
            ("--older-than", "30"), ("--types", "5"), ("--by-owner",), ("--cleanup",), ("--exclusive",),
        ):
            hint = run(binary, "scan", root, *option, status=1)
            assert "Space 16X Pro" in hint.stderr and not hint.stdout, hint.stderr
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
    # Pro adds each entry's newest modification time.
    dated = [entry.pop("newestModified", None) for entry in depth["entries"]]
    assert all(dated) if edition(binary) == "pro" else not any(dated), dated
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


def check_cleanup(binary):
    """Pro: --cleanup lists folders that tools recreate, found by name."""
    with tempfile.TemporaryDirectory(prefix="space16x-cleanup-") as directory:
        root = Path(directory).resolve()
        files = {
            "web/node_modules/pkg/index.js": 1_200_000,
            "web/node_modules/pkg/node_modules/dep.js": 1_100_000,
            "web/app.js": 2_000_000,
            "rust/Cargo.toml": 100,
            "rust/target/debug/app": 1_500_000,
            "notes/target/plan.txt": 1_500_000,
        }
        for name, size in files.items():
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(os.urandom(size))
        report = json.loads(run(binary, "scan", root, "--cleanup", "--json").stdout)
        found = [(str(Path(item["path"]).relative_to(root)), item["rule"]) for item in report["suggestions"]]
        assert found == [("web/node_modules", "node-modules"), ("rust/target", "rust-target")], found
        assert report["cleanupBytes"] == sum(item["allocatedBytes"] for item in report["suggestions"]), report
        assert report["suggestions"][0]["itemCount"] == 4 and "entries" not in report, report
        text = run(binary, "scan", root, "--cleanup")
        assert "Node packages" in text.stdout and "2 suggestions" in text.stdout, text.stdout
        assert "press u in the explorer" in text.stderr, text.stderr
        for invalid in (("--cleanup", "--top", "3"), ("--cleanup", "--types", "2"), ("--cleanup", "--cleanup")):
            assert run(binary, "scan", root, *invalid, status=1).stderr


def check_exclusive(binary):
    """Pro: --exclusive leaves out hard-linked files and blocks shared with clones."""
    with tempfile.TemporaryDirectory(prefix="space16x-exclusive-") as directory:
        root = Path(directory).resolve()
        (root / "own").mkdir()
        (root / "linked").mkdir()
        (root / "own" / "solo.bin").write_bytes(os.urandom(300_000))
        (root / "linked" / "first.bin").write_bytes(os.urandom(200_000))
        os.link(root / "linked" / "first.bin", root / "own" / "second.bin")
        report = json.loads(run(binary, "scan", root, "--exclusive", "--json").stdout)
        rows = {Path(row["path"]).name: row for row in report["entries"]}
        own = allocated_bytes(root / "own" / "solo.bin")
        assert rows["linked"]["exclusiveBytes"] == 0, rows
        assert rows["own"]["exclusiveBytes"] == own == report["exclusiveBytes"], report
        top = json.loads(run(binary, "scan", root, "--exclusive", "--top", "1", "--json").stdout)
        assert top["topFiles"][0]["exclusiveBytes"] == own, top
        text = run(binary, "scan", root, "--exclusive")
        assert text.stdout.splitlines()[0].split() == ["Size", "Exclusive", "Path"], text.stdout
        assert "hard links" in text.stderr, text.stderr
        if sys.platform == "darwin" and subprocess.run(
            ["cp", "-c", root / "own" / "solo.bin", root / "own" / "clone.bin"], capture_output=True
        ).returncode == 0:
            cloned = json.loads(run(binary, "scan", root, "--exclusive", "--json").stdout)
            rows = {Path(row["path"]).name: row for row in cloned["entries"]}
            # The original and its clone share every block, so neither frees them alone.
            assert rows["own"]["exclusiveBytes"] == 0 < rows["own"]["allocatedBytes"], rows
        for invalid in (("--exclusive", "--apparent"), ("--exclusive", "--by-owner"), ("--exclusive", "--exclusive")):
            assert run(binary, "scan", root, *invalid, status=1).stderr


def check_explain(binary):
    """Pro: explain shows the volume of a folder and the space a scan can't see."""
    with tempfile.TemporaryDirectory(prefix="space16x-explain-") as directory:
        folder = Path(directory).resolve()
        held = folder / "held.log"
        with held.open("wb") as stream:
            stream.write(os.urandom(65536))
            stream.flush()
            os.fsync(stream.fileno())
            held.unlink()
            plain = run(binary, "explain", folder).stdout
            report = json.loads(run(binary, "explain", folder, "--json").stdout)
    assert plain.startswith("Volume      ") and "\nFree        " in plain, plain
    assert report["schemaVersion"] == 3 and report["edition"] == "pro", report
    volume = report["volume"]
    assert volume["totalBytes"] > 0 and volume["usedBytes"] <= volume["totalBytes"], volume
    assert folder.is_relative_to(volume["path"]) or sys.platform == "darwin", volume
    if sys.platform == "darwin":
        assert isinstance(report["snapshots"], list), report
        if volume["type"] == "apfs":
            assert volume["path"] in [item["path"] for item in report["containerVolumes"]], report
    else:
        # This process holds the deleted file open, so its space is still in use.
        files = [item for item in report["deletedOpenFiles"] if item["path"] == str(held)]
        assert files and files[0]["pid"] == os.getpid() and files[0]["apparentBytes"] == 65536, report
        assert "deleted file" in plain and str(held) in plain, plain
    for invalid in (("explain", "a", "b"), ("explain", "--all")):
        assert run(binary, *invalid, status=1).stderr
    assert run(binary, "explain", "/no/such/space16x/path", status=1).stderr


def build_modified_fixture(root):
    """Creates fixtures/modified-times.json below root and returns the table."""
    table = json.loads(
        (Path(__file__).parent.parent / "fixtures" / "modified-times.json").read_text(encoding="utf-8")
    )
    for folder in table["folders"]:
        (root / folder).mkdir(parents=True, exist_ok=True)
    for file in table["files"]:
        path = root / file["path"]
        path.parent.mkdir(parents=True, exist_ok=True)
        if "hardLinkTo" in file:
            os.link(root / file["hardLinkTo"], path)
        else:
            path.write_bytes(os.urandom(5000))
            os.utime(path, (file["modified"], file["modified"]))
    return table


def check_modified(binary):
    """Pro: JSON reports newest modification times, and --older-than keeps older files."""
    with tempfile.TemporaryDirectory(prefix="space16x-modified-") as directory:
        root = Path(directory).resolve()
        table = build_modified_fixture(root)
        report = json.loads(run(binary, "scan", root, "--depth", "3", "--json").stdout)
        assert report["newestModified"] == table["newest"][""], report
        rows = {str(Path(row["path"]).relative_to(root)): row for row in report["entries"]}
        for path, newest in table["newest"].items():
            if path:
                assert rows[path].get("newestModified") == newest, (path, rows[path])
        top = json.loads(run(binary, "scan", root, "--top", "1", "--json").stdout)
        assert "newestModified" in top["topFiles"][0], top

        # Any cutoff between the fixture's 2022 and 2023 files keeps the same entries.
        days = (int(time.time()) - 1_675_000_000) // 86_400
        old = json.loads(run(binary, "scan", root, "--depth", "3", "--older-than", days, "--json").stdout)
        kept = [str(Path(row["path"]).relative_to(root)) for row in old["entries"]]
        assert sorted(kept) == sorted(table["olderThan"]["kept"]), kept
        assert old["olderThanDays"] == days and old["allocatedBytes"] < report["allocatedBytes"], old
        text = run(binary, "scan", root, "--older-than", days)
        assert "fresh.bin" not in text.stdout and "docs" in text.stdout, text.stdout
        assert f"more than {days} days ago" in text.stderr, text.stderr
        for invalid in (("--older-than",), ("--older-than", "-3"), ("--older-than", "1", "--export", "-")):
            assert run(binary, "scan", root, *invalid, status=1).stderr


def check_types(binary):
    """Pro: --types lists the space each file extension uses."""
    with tempfile.TemporaryDirectory(prefix="space16x-types-") as directory:
        root = Path(directory).resolve()
        (root / "clips").mkdir()
        for name, size in (("clips/a.MOV", 300_000), ("clips/b.mov", 200_000), ("notes.txt", 9_000),
                           ("Makefile", 5_000), (".bashrc", 5_000), ("x.tar.gz", 40_000)):
            (root / name).write_bytes(os.urandom(size))
        os.link(root / "clips" / "a.MOV", root / "clips" / "alias.mov")
        report = json.loads(run(binary, "scan", root, "--types", 2, "--json").stdout)
        assert report["typeLimit"] == 2 and report["typeCount"] == 4, report
        assert [row["type"] for row in report["types"]] == ["mov", "gz"], report
        mov = report["types"][0]
        assert mov["fileCount"] == 2, mov
        assert mov["allocatedBytes"] == allocated_bytes(root / "clips" / "a.MOV") + allocated_bytes(root / "clips" / "b.mov")
        assert "entries" not in report and report["allocatedBytes"] >= mov["allocatedBytes"], report
        text = run(binary, "scan", root, "--types", 1).stdout.splitlines()
        assert text[0].split() == ["Size", "Files", "Type"], text
        assert text[1].endswith(".mov") and "(3 other types)" in text[2] and text[3].endswith("[total]"), text
        everything = json.loads(run(binary, "scan", root, "--types", 10, "--json").stdout)
        assert "" in [row["type"] for row in everything["types"]], everything
        for invalid in (("--types",), ("--types", "2", "--depth", "1"), ("--top", "1", "--types", "1")):
            assert run(binary, "scan", root, *invalid, status=1).stderr

        # --by-owner sums each user's files and folders. This user owns all of them.
        owners = json.loads(run(binary, "scan", root, "--by-owner", "--json").stdout)
        assert len(owners["owners"]) == 1 and "entries" not in owners, owners
        owner = owners["owners"][0]
        assert owner["uid"] == os.getuid() and owner["owner"] == pwd.getpwuid(os.getuid()).pw_name, owner
        assert owner["itemCount"] == owners["itemCount"] and owner["allocatedBytes"] == owners["allocatedBytes"]
        text = run(binary, "scan", root, "--by-owner").stdout.splitlines()
        assert text[0].split() == ["Size", "Items", "Owner"] and text[1].endswith(owner["owner"]), text
        assert text[-1].endswith("[total]"), text
        for invalid in (("--by-owner", "--types", "1"), ("--by-owner", "--top", "1"), ("--by-owner", "--by-owner")):
            assert run(binary, "scan", root, *invalid, status=1).stderr


def check_history(binary):
    """Pro: history keeps scans only when asked, lists them, and compares with them."""
    with tempfile.TemporaryDirectory(prefix="space16x-history-") as directory:
        base = Path(directory).resolve()
        state = base / "state"
        env = {**os.environ, "XDG_STATE_HOME": str(state)}
        root = base / "data"
        (root / "grow").mkdir(parents=True)
        (root / "grow" / "a.bin").write_bytes(os.urandom(10_000))
        (root / "skip.log").write_bytes(os.urandom(50_000))

        # Nothing is written until a scan is saved.
        empty = run(binary, "history", env=env)
        assert empty.stdout == "" and "no saved scans" in empty.stderr and not state.exists(), empty
        none = run(binary, "history", "diff", root, env=env, status=1)
        assert "No saved scan" in none.stderr, none.stderr

        saved = run(binary, "history", "save", root, "--exclude", "*.log", env=env)
        assert saved.stdout.startswith(f"Saved {root} ("), saved.stdout
        folder = state / "space16x" / "history"
        assert folder.stat().st_mode & 0o777 == 0o700
        listing = json.loads(run(binary, "history", root, "--json", env=env).stdout)
        assert listing["directory"]["path"] == str(folder) and len(listing["scans"]) == 1, listing
        scan = listing["scans"][0]
        assert scan["scope"] == str(root) and scan["itemCount"] == 2 and not scan["isPartial"], scan
        export = Path(scan["path"]["path"])
        assert export.parent == folder and export.stat().st_mode & 0o777 == 0o600
        assert json.loads(export.read_text(encoding="utf-8"))[2]["space16x_excludes"] == ["*.log"]
        assert json.loads(run(binary, "history", base, "--json", env=env).stdout)["scans"] == []
        text = run(binary, "history", env=env).stdout.splitlines()
        assert text[0].split() == ["Saved", "Size", "Items", "Path"] and text[1].endswith(str(root)), text

        # diff compares the folder now with the saved scan, using its excludes.
        (root / "grow" / "a.bin").write_bytes(os.urandom(400_000))
        (root / "more.log").write_bytes(os.urandom(90_000))
        change = json.loads(run(binary, "history", "diff", root, "--json", env=env).stdout)
        assert [Path(row["path"]).name for row in change["entries"]] == ["grow"], change
        assert change["old"]["allocatedBytes"] == scan["allocatedBytes"], change
        assert run(binary, "history", "diff", root, "--since", 2, env=env, status=1).stderr
        plain = run(binary, "history", "diff", root, env=env)
        assert "comparing with the scan saved" in plain.stderr and "[total]" in plain.stdout, plain
        for invalid in (("history", "save", "--json"), ("history", "list", "--since", "1"), ("history", "a", "b")):
            assert run(binary, *invalid, env=env, status=1).stderr


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
        check_modified(binary)
        check_types(binary)
        check_history(binary)
        check_explain(binary)
        check_cleanup(binary)
        check_exclusive(binary)
    print("CLI acceptance checks passed.")


if __name__ == "__main__":
    main()
