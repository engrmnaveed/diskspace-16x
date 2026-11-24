"""A synthetic home folder for the user guide's screenshots and videos.

Every file is generated, so no real user path, name or content appears in the media.
Sizes are decimal megabytes, as DiskUse16x shows them, and ages are days since the file
last changed, which Pro's Modified column and older-files filter show.
"""

import os
import subprocess
import time

MB = 1_000_000

# (size in MB, age in days)
FILES = {
    "Movies/Holiday 2025.mov": (420, 400),
    "Movies/Wedding edit.mp4": (240, 210),
    "Movies/Screen Recording.mov": (85, 3),
    "Downloads/project-backup.zip": (130, 75),
    "Downloads/brochure.pdf": (12, 20),
    "Pictures/Exports/poster.png": (26, 45),
    "Documents/Taxes 2025.pdf": (4, 90),
    "Library/Caches/com.example.browser/Cache.db": (150, 1),
    "Library/Caches/com.example.maps/tiles.bin": (45, 2),
    # An Xcode archive, which Pro's cleanup suggestions list to review.
    "Library/Developer/Xcode/Archives/Budget 2026-06-01.xcarchive/Products/Budget": (48, 170),
    # Build output and packages that tools recreate, which Pro's cleanup suggestions list.
    "Developer/budget-app/Cargo.toml": (0, 30),
    "Developer/budget-app/target/debug/budget-app": (60, 2),
    "Developer/website/package.json": (0, 30),
}
# Many files of one kind give the Items column and Pro's file types something to show.
SERIES = [
    ("Pictures/Photos/IMG_{:04}.heic", 60, 3, lambda index: 20 + index * 11),
    ("Music/Podcasts/episode-{:02}.mp3", 8, 18, lambda index: 120 + index * 7),
    ("Documents/Thesis/draft-{}.docx", 5, 2, lambda index: 300 - index * 20),
    ("Desktop/Screenshot {}.png", 12, 1.5, lambda index: 5 + index),
    ("Developer/website/node_modules/pkg-{:03}/index.js", 400, 0.15, lambda index: 30),
]
# Copies with the same contents, which Pro's Duplicates finds.
COPIES = {
    "Downloads/Wedding edit (1).mp4": "Movies/Wedding edit.mp4",
    "Documents/brochure.pdf": "Downloads/brochure.pdf",
}
# An APFS clone shares its blocks with the original, so Duplicates marks it as freeing nothing.
CLONES = {"Desktop/Wedding edit - review.mp4": "Movies/Wedding edit.mp4"}
# Files that appear after the baseline scan, which Pro's Compare and diff show as growth.
GROWTH = {
    "Downloads/Installer.dmg": (210, 1),
    "Developer/budget-app/target/debug/incremental.bin": (110, 0),
}
# A file that the baseline has and the folder no longer does.
REMOVED = "Documents/Thesis/draft-0.docx"
# A file added while the explorer is open, which the rescan scene picks up.
RESCAN = ("Downloads/Screen Recording 2.mov", 64)

_BLOCK = os.urandom(MB)


def _write(path, megabytes, age_days):
    """Write unique contents, so only the intended copies match as duplicates."""
    path.parent.mkdir(parents=True, exist_ok=True)
    size = round(megabytes * MB)
    with path.open("wb") as file:
        head = os.urandom(min(size, 65_536))
        file.write(head)
        remaining = size - len(head)
        while remaining > 0:
            chunk = _BLOCK[: min(remaining, MB)]
            file.write(chunk)
            remaining -= len(chunk)
    _age(path, age_days)


def _age(path, days):
    modified = time.time() - days * 86_400
    os.utime(path, (modified, modified))


def create(home, cli):
    """Create the folder, and return a baseline scan saved before it grew."""
    for relative, (megabytes, age) in FILES.items():
        _write(home / relative, megabytes, age)
    for pattern, count, megabytes, age in SERIES:
        for index in range(count):
            _write(home / pattern.format(index + 1), megabytes, age(index))
    for copy, original in COPIES.items():
        (home / copy).write_bytes((home / original).read_bytes())
        _age(home / copy, 15)
    for clone, original in CLONES.items():
        subprocess.run(["cp", "-c", home / original, home / clone], check=True)
        _age(home / clone, 12)
    _write(home / REMOVED, 2, 320)

    baseline = home.parent / "last-week.json"
    with baseline.open("wb") as output:
        subprocess.run([cli, "scan", home, "--export", "-"], stdout=output, stderr=subprocess.DEVNULL, check=True)

    for relative, (megabytes, age) in GROWTH.items():
        _write(home / relative, megabytes, age)
    (home / REMOVED).unlink()
    return baseline


def add_rescan_file(home):
    path = home / RESCAN[0]
    _write(path, RESCAN[1], 0)
    return path

