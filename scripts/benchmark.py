#!/usr/bin/env python3
"""Measure wall time and maximum resident memory on a reproducible tree."""

import argparse
import json
import os
from pathlib import Path
import platform
import re
from subprocess import DEVNULL, check_output, run
import tempfile
import time


DIRECTORIES = 1_000
FILES_PER_DIRECTORY = 100
PAYLOAD = bytes(512)


def create_fixture(root):
    allocated_bytes = 0
    for group_index in range(100):
        group = root / f"group-{group_index:03}"
        group.mkdir()
        for directory_index in range(10):
            directory = group / f"folder-{directory_index:03}"
            directory.mkdir()
            for file_index in range(FILES_PER_DIRECTORY):
                path = directory / f"file-{file_index:03}.bin"
                path.write_bytes(PAYLOAD)
                allocated_bytes += path.stat().st_blocks * 512
    return allocated_bytes


def measure(binary, root, os_name, expected_bytes):
    scan = [binary, "scan", str(root), "--top", "20", "--json"]
    with tempfile.TemporaryFile() as output:
        started = time.perf_counter()
        actions = [(os.POSIX_SPAWN_DUP2, output.fileno(), 1)]
        child = os.posix_spawn(binary, scan, os.environ, file_actions=actions)
        _, status, usage = os.wait4(child, 0)
        seconds = time.perf_counter() - started
        if os.waitstatus_to_exitcode(status) != 0:
            raise RuntimeError("The timed scan failed.")
        output.seek(0)
        report = json.load(output)
    if report["isPartial"] or report["allocatedBytes"] != expected_bytes:
        raise RuntimeError("The scan did not include the full benchmark fixture.")
    # Darwin reports ru_maxrss in bytes. Linux reports it in KiB.
    peak_bytes = usage.ru_maxrss * (1 if os_name == "Darwin" else 1024)
    return {"wallSeconds": round(seconds, 4), "peakResidentBytes": peak_bytes}


def hardware_name(os_name):
    if os_name == "Darwin":
        return check_output(["sysctl", "-n", "machdep.cpu.brand_string"], text=True).strip()
    match = re.search(r"^model name\s*:\s*(.*)", Path("/proc/cpuinfo").read_text(), re.MULTILINE)
    return match.group(1) if match else platform.machine()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path, help="path to a release build of space16x")
    parser.add_argument("--fixture-parent", type=Path, help="directory on the filesystem to benchmark")
    args = parser.parse_args()
    binary = str(args.binary.resolve(strict=True))
    os_name = platform.system()
    if os_name not in ("Darwin", "Linux"):
        parser.error("The benchmark supports macOS and Linux.")
    with tempfile.TemporaryDirectory(prefix="space16x-bench-", dir=args.fixture_parent) as path:
        root = Path(path).resolve()
        allocated_bytes = create_fixture(root)
        run([binary, "scan", str(root), "--top", "20", "--json"], stdout=DEVNULL, check=True)
        runs = [measure(binary, root, os_name, allocated_bytes) for _ in range(3)]
        if os_name == "Darwin":
            os_version = check_output(["sw_vers", "-productVersion"], text=True).strip()
        else:
            os_version = platform.freedesktop_os_release()["PRETTY_NAME"]
        print(json.dumps({
            "cpu": hardware_name(os_name),
            "architecture": platform.machine(),
            "operatingSystem": f"{os_name} {os_version}",
            "filesystemMount": check_output(["df", "-P", str(root)], text=True).splitlines()[-1],
            "fixture": {
                "groupDirectories": 100,
                "leafDirectories": DIRECTORIES,
                "files": DIRECTORIES * FILES_PER_DIRECTORY,
                "payloadBytesPerFile": len(PAYLOAD),
                "allocatedBytes": allocated_bytes,
            },
            "warmups": 1,
            "runs": runs
        }, indent=2))


if __name__ == "__main__":
    main()
