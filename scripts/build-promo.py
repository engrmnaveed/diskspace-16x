#!/usr/bin/env python3
"""Build the Space 16X Short and README preview locally on macOS."""

import argparse
import array
from importlib.util import find_spec
import json
import math
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tempfile
import time
import wave


ROOT = Path(__file__).resolve().parents[1]
PROMO = ROOT / "scripts" / "promo"
SHOTS = [
    ("opening", 75), ("launch", 30), ("folders", 75), ("drill", 60), ("rank", 75), ("trash", 75),
    ("report", 60), ("carry", 90), ("map", 75), ("share", 75), ("duplicates", 90), ("end", 90),
]
FPS = 30
FIXTURE = {
    "Exports/launch.mov": 32,
    "Exports/teaser.mp4": 14,
    "Media/interview.wav": 16,
    "Media/b-roll.mov": 11,
    "Backups/archive.zip": 9,
    "Assets/texture.psd": 5,
    "Assets/cover.png": 1,
    # Folders that tools recreate, which Pro's cleanup suggestions list.
    "App/Cargo.toml": 0,
    "App/target/debug/app": 8,
    "Site/package.json": 0,
    "Site/node_modules/react/react.js": 6,
}
# A copy saved twice, which the Mac app's Duplicates scene finds.
COPIES = {"Backups/teaser-final.mp4": "Exports/teaser.mp4"}
# Days since each file last changed, which Pro's Modified column shows.
AGES = {
    "Exports/launch.mov": 0,
    "Exports/teaser.mp4": 2,
    "Media/interview.wav": 9,
    "Media/b-roll.mov": 23,
    "Backups/archive.zip": 410,
    "Assets/texture.psd": 61,
    "Assets/cover.png": 61,
}

ICONS = 48


def create_fixture(root):
    for relative, mib in FIXTURE.items():
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open("wb") as file:
            for _ in range(mib):
                file.write(os.urandom(1024 * 1024))
        modified = time.time() - AGES.get(relative, 0) * 86_400
        os.utime(path, (modified, modified))
    for copy, original in COPIES.items():
        (root / copy).write_bytes((root / original).read_bytes())
    # Many small files give the item-count sort something to show.
    folder = root / "Assets" / "icons"
    folder.mkdir()
    for index in range(ICONS):
        icon = folder / f"icon-{index:02}.png"
        icon.write_bytes(os.urandom(2048))
        modified = time.time() - 61 * 86_400
        os.utime(icon, (modified, modified))


def capture(binary, work):
    from promo.terminal_capture import capture_explorer

    projects = work / "Projects"
    create_fixture(projects)
    # The import scene browses this folder's scan as if it came from a server's /srv.
    server = json.loads(subprocess.check_output([binary, "scan", projects, "--export", "-"]))
    server[3][0]["name"] = "/srv"
    saved = work / "server.json"
    saved.write_text(json.dumps(server), encoding="utf-8")
    explorer = capture_explorer(binary, projects, saved)
    explorer_path = work / "explorer.json"
    explorer_path.write_text(json.dumps(explorer), encoding="utf-8")
    # The plain-text CLI prints absolute paths. Show them relative to the folder on screen.
    report = subprocess.check_output(
        [binary, "scan", "--cleanup"], cwd=projects, text=True, stderr=subprocess.DEVNULL,
    ).replace(f"{projects}/", "")
    report_path = work / "report.txt"
    report_path.write_text(report, encoding="utf-8")

    core_sources = sorted((ROOT / "Sources" / "SpaceCore").glob("*.swift"))
    views = sorted((ROOT / "Sources" / "SpaceApp").glob("*.swift"))
    compiler = ["swiftc", "-swift-version", "6", "-O", "-module-cache-path",
                ROOT / ".build" / "promo-module-cache"]
    subprocess.run([
        *compiler, "-module-name", "SpaceCore", "-emit-module", "-emit-module-path",
        work / "SpaceCore.swiftmodule", "-emit-library", "-static", "-o", work / "libSpaceCore.a",
        *core_sources,
    ], check=True)
    subprocess.run([
        *compiler, "-module-name", "SpacePro", "-I", work, "-emit-module", "-emit-module-path",
        work / "SpacePro.swiftmodule", "-emit-library", "-static", "-o", work / "libSpacePro.a",
        *sorted((ROOT / "Sources" / "SpacePro").glob("*.swift")),
    ], check=True)
    executable = work / "capture"
    subprocess.run([
        *compiler, "-I", work, "-L", work, "-lSpaceCore", "-lSpacePro", "-o", executable,
        *sorted(PROMO.glob("*.swift")), *(path for path in views if path.name != "Space16XApp.swift"),
    ], check=True)
    subprocess.run([
        executable, projects, work, report_path, explorer_path, ROOT / "Branding" / "AppIcon.icns",
    ], check=True)


def soundtrack(path, duration):
    """Synthesize a quiet arpeggio and a soft downbeat for each scene change."""
    rate = 48000
    samples = array.array("f", [0]) * round(rate * duration)

    def note(start, length, midi, volume):
        offset = round(start * rate)
        radians = math.tau * 440 * 2 ** ((midi - 69) / 12) / rate
        for index in range(min(round(rate * length), len(samples) - offset)):
            time = index / rate
            envelope = min(1, time / 0.012) * (1 - time / length) ** 2
            wave_value = math.sin(index * radians) + 0.21 * math.sin(index * radians * 2)
            samples[offset + index] += volume * envelope * wave_value

    chords = [(45, 57, 60, 64), (41, 57, 60, 65), (48, 55, 60, 64), (43, 55, 59, 62)]
    for beat in range(round(duration * 2)):
        chord = chords[(beat // 4) % len(chords)]
        note(beat / 2, 0.72, chord[1 + beat % 3], 0.13 if beat % 2 == 0 else 0.08)
        if beat % 4 == 0:
            note(beat / 2, 1.5, chord[0], 0.15)
    offset = 0
    for name, frames in SHOTS:
        if name != "launch":
            note(offset / FPS, 0.17, 32, 0.13)
        offset += frames
    pcm = array.array("h", (
        round(sample * 32767 * min(1, (len(samples) - index) / (rate * 0.7)))
        for index, sample in enumerate(samples)
    ))
    if sys.byteorder == "big":
        pcm.byteswap()
    with wave.open(str(path), "wb") as output:
        output.setparams((1, 2, rate, 0, "NONE", "not compressed"))
        output.writeframes(pcm.tobytes())


def encode(work):
    duration = sum(frames for _, frames in SHOTS) / FPS
    if not 20 <= duration <= 30:
        raise ValueError(f"Short must be 20–30 seconds. The current cut is {duration:g} seconds.")
    ffmpeg = ["ffmpeg", "-hide_banner", "-loglevel", "error", "-nostdin", "-y", "-filter_complex_threads", "2"]
    inputs = []
    filters = []
    for index, (name, frames) in enumerate(SHOTS):
        inputs.extend(["-i", work / f"{name}.png"])
        # Scale up first to keep the small vertical pan smooth at 30 fps.
        zoom = f"1+0.035*on/{frames - 1}" if name in ("opening", "map", "share", "compare", "end") else "1"
        filters.append(
            f"[{index}:v]scale=2160:3840:out_color_matrix=bt709,format=yuv420p,"
            f"zoompan=z='{zoom}':x='(iw-iw/zoom)/2':y='(ih-ih/zoom)/2':"
            f"d={frames}:s=1080x1920:fps={FPS}[shot{index}]"
        )
    sequence = "".join(f"[shot{index}]" for index in range(len(SHOTS)))
    filters.append(f"{sequence}concat=n={len(SHOTS)}:v=1:a=0[video]")
    audio = work / "soundtrack.wav"
    soundtrack(audio, duration)
    subprocess.run([
        *ffmpeg, *inputs, "-i", audio, "-filter_complex", ";".join(filters),
        "-map", "[video]", "-map", f"{len(SHOTS)}:a", "-c:v", "libx264", "-preset", "fast",
        "-crf", "19", "-colorspace", "bt709", "-color_primaries", "bt709", "-color_trc", "bt709",
        "-c:a", "aac", "-b:a", "128k", "-movflags", "+faststart", work / "space16x-short.mp4",
    ], check=True)
    subprocess.run([
        *ffmpeg, "-framerate", "1/2", "-i", work / "preview-%d.png",
        "-filter_complex", "split[p][v];[p]palettegen[pal];[v][pal]paletteuse=dither=bayer",
        "-loop", "0", "-final_delay", "200", work / "space16x-preview.gif",
    ], check=True)


def cli_path(parser, binary):
    if binary is not None:
        binary = binary.resolve()
        if not binary.is_file() or not os.access(binary, os.X_OK):
            parser.error(f"CLI is not an executable file: {binary}")
        return binary
    cargo = shutil.which("cargo") or Path.home() / ".cargo" / "bin" / "cargo"
    if not os.access(cargo, os.X_OK):
        parser.error("Install Rustup with Cargo or pass --cli PATH to a built Space 16X CLI.")
    # The promo shows Pro's analysis, so it builds the Pro edition.
    target = ROOT / "cli" / "target" / "promo"
    subprocess.run(
        [cargo, "build", "--release", "--locked", "--features", "pro", "--target-dir", target],
        cwd=ROOT / "cli", check=True,
    )
    return target / "release" / "space16x"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, metavar="PATH", help="reuse a built Space 16X CLI")
    parser.add_argument("--output", type=Path, default=ROOT / "dist" / "promo", help="output directory")
    args = parser.parse_args()
    if platform.system() != "Darwin":
        parser.error("macOS is required to render the native desktop views.")
    if not shutil.which("ffmpeg") or not shutil.which("swiftc"):
        parser.error("Install FFmpeg with libx264 and the Swift command line tools.")
    if find_spec("pyte") is None:
        parser.error("Install Python dependencies with: python -m pip install -r scripts/promo/requirements.txt")

    binary = cli_path(parser, args.cli)
    args.output.mkdir(parents=True, exist_ok=True)
    # Keep both scans confined to a fresh synthetic directory with no user paths.
    with tempfile.TemporaryDirectory(prefix="s16x-", dir="/private/tmp") as directory:
        work = Path(directory)
        print("Rendering native desktop views and CLI output…", flush=True)
        capture(binary, work)
        print("Encoding the Short and README preview…", flush=True)
        encode(work)
        for name in ("space16x-short.mp4", "space16x-preview.gif"):
            destination = args.output / name
            shutil.copyfile(work / name, destination)
            print(destination.resolve())


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError, ValueError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"Promo build failed: {error}") from error
