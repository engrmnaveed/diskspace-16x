#!/usr/bin/env python3
"""Build the user guide's screenshots, GIFs and walkthrough videos locally on macOS."""

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
import wave


ROOT = Path(__file__).resolve().parents[1]
GUIDE = ROOT / "scripts" / "guide"
sys.path[:0] = [str(GUIDE), str(ROOT / "scripts")]
FPS = 30
FFMPEG = ["ffmpeg", "-hide_banner", "-loglevel", "error", "-nostdin", "-y"]


def build_cli(parser, given, pro):
    """Return a Space 16X CLI of the requested edition, building it if none was given."""
    if given is not None:
        binary = given.resolve()
        if not binary.is_file() or not os.access(binary, os.X_OK):
            parser.error(f"CLI is not an executable file: {binary}")
        return binary
    cargo = shutil.which("cargo") or Path.home() / ".cargo" / "bin" / "cargo"
    if not os.access(cargo, os.X_OK):
        parser.error("Install Rustup with Cargo or pass --cli and --cli-pro.")
    target = ROOT / "cli" / "target" / ("guide-pro" if pro else "guide-basic")
    features = ["--features", "pro"] if pro else []
    subprocess.run([cargo, "build", "--release", "--locked", *features, "--target-dir", target],
                   cwd=ROOT / "cli", check=True)
    return target / "release" / "space16x"


def build_capture(work):
    """Compile GuideCapture with the shipping SwiftUI views, as the promo's capture does."""
    sources = work / "app"
    sources.mkdir()
    for path in sorted((ROOT / "Sources" / "SpaceApp").glob("*.swift")):
        if path.name != "Space16XApp.swift":
            shutil.copy(path, sources / path.name)
    # The start screen lists made-up volumes, so this copy lets the capture set them.
    workspace = sources / "ScanWorkspace.swift"
    text = workspace.read_text(encoding="utf-8")
    declaration = "private(set) var volumes: [VolumeSummary]"
    if declaration not in text:
        raise RuntimeError("ScanWorkspace.volumes changed. Update build_capture in scripts/build-guide-media.py.")
    workspace.write_text(text.replace(declaration, "var volumes: [VolumeSummary]"), encoding="utf-8")

    compiler = ["swiftc", "-swift-version", "6", "-O", "-module-cache-path", ROOT / ".build" / "guide-module-cache"]
    subprocess.run([
        *compiler, "-module-name", "SpaceCore", "-emit-module", "-emit-module-path", work / "SpaceCore.swiftmodule",
        "-emit-library", "-static", "-o", work / "libSpaceCore.a", *sorted((ROOT / "Sources" / "SpaceCore").glob("*.swift")),
    ], check=True)
    subprocess.run([
        *compiler, "-module-name", "SpacePro", "-I", work, "-emit-module", "-emit-module-path",
        work / "SpacePro.swiftmodule", "-emit-library", "-static", "-o", work / "libSpacePro.a",
        *sorted((ROOT / "Sources" / "SpacePro").glob("*.swift")),
    ], check=True)
    executable = work / "guide-capture"
    subprocess.run([
        *compiler, "-I", work, "-L", work, "-lSpaceCore", "-lSpacePro", "-o", executable,
        *sorted(GUIDE.glob("*.swift")), *sorted(sources.glob("*.swift")),
    ], check=True)
    return executable


def shrink_png(source, destination, palette):
    """Recompress a screenshot, as 256 colors when `palette` is set.

    Window screenshots keep their look in 256 colors at about half the size. Terminal
    screenshots stay in full color, since a palette tints their thin lines.
    """
    variants = {"full": "format=rgb24"}
    if palette:
        variants["palette"] = "split[a][b];[a]palettegen=max_colors=256:stats_mode=full[p];[b][p]paletteuse=dither=none"
    candidates = [source]
    for name, filters in variants.items():
        output = source.with_suffix(f".{name}.png")
        subprocess.run([*FFMPEG, "-i", source, "-filter_complex", filters, "-compression_level", "9",
                        "-pred", "mixed", output], check=True)
        candidates.append(output)
    shutil.copyfile(min(candidates, key=lambda path: path.stat().st_size), destination)


def concat_list(steps, path):
    """An FFmpeg concat list that shows each image for its duration."""
    lines = ["ffconcat version 1.0"]
    for image, seconds in steps:
        lines += [f"file '{image}'", f"duration {seconds:.3f}"]
    # The concat demuxer needs the last image twice to honor its duration.
    lines.append(f"file '{steps[-1][0]}'")
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def encode_gif(steps, work, destination):
    listing = work / f"{destination.stem}.txt"
    concat_list(steps, listing)
    subprocess.run([
        *FFMPEG, "-f", "concat", "-safe", "0", "-i", listing, "-filter_complex",
        "split[a][b];[a]palettegen=max_colors=128:stats_mode=full[p];"
        "[b][p]paletteuse=dither=bayer:bayer_scale=4:diff_mode=rectangle",
        "-loop", "0", destination,
    ], check=True)


def soundtrack(path, duration):
    """A quiet, slow pad of original synthesized chords, so the video isn't silent."""
    rate = 24_000
    chord_seconds = 4.0
    chords = [(45, 57, 60, 64), (41, 57, 60, 65), (48, 55, 60, 64), (43, 55, 59, 62)]
    loop = array.array("f", [0.0]) * round(rate * chord_seconds * len(chords))
    for index, chord in enumerate(chords):
        start = round(index * chord_seconds * rate)
        length = round((chord_seconds + 1.0) * rate)
        for voice, midi in enumerate(chord):
            step = math.tau * 440 * 2 ** ((midi - 69) / 12) / rate
            volume = 0.07 if voice == 0 else 0.04
            for offset in range(length):
                time = offset / rate
                envelope = min(1.0, time / 1.2) * min(1.0, max(0.0, (chord_seconds + 1.0 - time) / 1.6))
                value = math.sin(offset * step) + 0.18 * math.sin(2 * offset * step)
                loop[(start + offset) % len(loop)] += volume * envelope * value
    total = round(rate * duration)
    samples = (loop * (total // len(loop) + 1))[:total]
    pcm = array.array("h", (
        round(32767 * value * min(1.0, index / (rate * 1.5), (total - index) / (rate * 2.5)))
        for index, value in enumerate(samples)
    ))
    if sys.byteorder == "big":
        pcm.byteswap()
    with wave.open(str(path), "wb") as output:
        output.setparams((1, 2, rate, 0, "NONE", "not compressed"))
        output.writeframes(pcm.tobytes())


def encode_video(steps, work, destination):
    listing = work / f"{destination.stem}.txt"
    concat_list(steps, listing)
    duration = sum(seconds for _, seconds in steps)
    audio = work / f"{destination.stem}.wav"
    soundtrack(audio, duration)
    subprocess.run([
        *FFMPEG, "-f", "concat", "-safe", "0", "-i", listing, "-i", audio,
        "-vf", f"fps={FPS},scale=out_color_matrix=bt709:out_range=tv,format=yuv420p",
        "-c:v", "libx264", "-preset", "slow", "-crf", "20", "-tune", "stillimage",
        "-colorspace", "bt709", "-color_primaries", "bt709", "-color_trc", "bt709",
        "-c:a", "aac", "-b:a", "128k", "-t", f"{duration:.3f}", "-movflags", "+faststart", destination,
    ], check=True)
    return duration


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, metavar="PATH", help="reuse a built Basic CLI")
    parser.add_argument("--cli-pro", type=Path, metavar="PATH", help="reuse a built Pro CLI")
    parser.add_argument("--images", type=Path, default=ROOT / "docs" / "guide" / "images",
                        help="where to write the screenshots and GIFs")
    parser.add_argument("--output", type=Path, default=ROOT / "dist" / "guide",
                        help="where to write the walkthrough videos and their YouTube texts")
    parser.add_argument("--no-videos", action="store_true", help="skip the walkthrough videos")
    args = parser.parse_args()
    if platform.system() != "Darwin":
        parser.error("macOS is required to render the native desktop views.")
    if not shutil.which("ffmpeg") or not shutil.which("swiftc"):
        parser.error("Install FFmpeg with libx264 and the Swift command line tools.")
    if find_spec("pyte") is None:
        parser.error("Install Python dependencies with: python -m pip install -r scripts/promo/requirements.txt")

    import fixture
    import storyboard

    basic = build_cli(parser, args.cli, pro=False)
    pro = build_cli(parser, args.cli_pro, pro=True)
    args.images.mkdir(parents=True, exist_ok=True)
    # Everything the guide shows comes from a fresh synthetic folder with no user paths.
    with tempfile.TemporaryDirectory(prefix="s16x-", dir="/private/tmp") as directory:
        work = Path(directory)
        home = work / "Home"
        print("Creating the synthetic folder…", flush=True)
        baseline = fixture.create(home, basic)
        print("Capturing the terminal explorer and reports…", flush=True)
        frames = storyboard.capture_terminal(basic, pro, home, work)
        spec, gifs, videos = storyboard.build(frames, work, videos=not args.no_videos)
        spec_path = work / "spec.json"
        spec_path.write_text(json.dumps(spec), encoding="utf-8")
        print("Compiling the native views…", flush=True)
        capture = build_capture(work)
        print("Rendering the Mac app and composing images…", flush=True)
        subprocess.run([capture, home, baseline, spec_path, ROOT / "Branding" / "AppIcon.icns"], check=True)

        print("Writing screenshots and GIFs…", flush=True)
        for output in spec["outputs"]:
            if output["kind"] == "shot":
                source = Path(output["path"])
                shrink_png(source, args.images / source.name, palette="source" in output)
        for name, steps in gifs.items():
            encode_gif(steps, work, args.images / f"{name}.gif")
        for path in sorted(args.images.iterdir()):
            print(f"{path.stat().st_size / 1000:8.0f} KB  {path.relative_to(ROOT) if path.is_relative_to(ROOT) else path}")

        if videos:
            print("Encoding the walkthrough videos…", flush=True)
            args.output.mkdir(parents=True, exist_ok=True)
            for name, plan in videos.items():
                destination = args.output / f"space16x-{name}-walkthrough.mp4"
                duration = encode_video(plan["steps"], work, destination)
                (args.output / f"space16x-{name}-youtube.txt").write_text(
                    storyboard.youtube_text(name, plan["chapters"], duration), encoding="utf-8",
                )
                print(f"{destination.resolve()}  {storyboard.timestamp(duration)}")


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError, ValueError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"Guide media build failed: {error}") from error
