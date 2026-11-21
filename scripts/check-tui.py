#!/usr/bin/env python3
"""Exercise the packaged explorer through a pseudo-terminal."""

import os
from pathlib import Path
import re
import sys
import tempfile
import termios
import time
import tty

from tui_terminal import Terminal


def check(binary, fixture):
    with Terminal([binary, fixture]) as terminal:
        terminal.expect(b"nested-alpha")
        start = len(terminal.output)
        terminal.send(b"\r")
        terminal.expect(b"payload.bin", start)

        # File details pair the decimal size with the binary size and exact bytes.
        start = len(terminal.output)
        terminal.send(b"\r")
        terminal.expect(" KiB · ".encode(), start)
        terminal.expect(b" bytes)", start)

        start = len(terminal.output)
        terminal.send(b"\x7f")
        terminal.expect(b"payload.bin", start)

        start = len(terminal.output)
        terminal.send(b"\x7f")
        terminal.expect(b"nested-alpha", start)

        start = len(terminal.output)
        terminal.send(b"f")
        terminal.expect(b"shown (up to 100)", start)
        terminal.expect(b"payload.bin", start)

        start = len(terminal.output)
        terminal.resize(52, 16)
        terminal.expect(b"payload.bin", start)
        terminal.send(b"q")
        terminal.finish(0)

    with Terminal([binary, fixture]) as terminal:
        terminal.expect(b"nested-alpha")
        assert termios.tcgetattr(terminal.slave)[tty.LFLAG] & termios.ICANON == 0
        terminal.send(b"\x03")
        terminal.finish(130)


def uses_color(output):
    """Whether any SGR sequence sets a foreground or background color."""
    for parameters in re.findall(rb"\x1b\[([0-9;]*)m", bytes(output)):
        for code in parameters.split(b";"):
            value = int(code or 0)
            if 30 <= value <= 38 or 40 <= value <= 48 or 90 <= value <= 107:
                return True
    return False


def check_sort_size_and_color(binary, fixture):
    environment = {key: value for key, value in os.environ.items() if key != "NO_COLOR"}
    with Terminal([binary, fixture], env=environment) as terminal:
        terminal.expect(b"nested-alpha")
        terminal.expect(b"Items")
        assert uses_color(terminal.output), "the explorer should use color by default"

        terminal.press(b"s", b"by items")
        terminal.press(b"a", b"apparent")
        terminal.send(b"q")
        terminal.finish(0)

    for command, env in (
        ([binary, "--no-color", fixture], environment),
        ([binary, fixture], {**environment, "NO_COLOR": "1"}),
    ):
        with Terminal(command, env=env) as terminal:
            terminal.expect(b"nested-alpha")
            terminal.send(b"q")
            terminal.finish(0)
            assert not uses_color(terminal.output), f"{command} printed colors"
            # The selection stays visible in reverse video.
            assert b"\x1b[7m" in terminal.output


def check_trash(binary):
    """Mark, decline, and confirm Move to Trash, and check that read-only mode refuses it.

    On Linux the FreeDesktop Trash goes to a scratch XDG_DATA_HOME. On macOS the item
    goes to the real Trash, so a local run stops before confirming unless CI is set.
    """
    with tempfile.TemporaryDirectory(prefix="space16x-trash-") as directory:
        root = Path(directory).resolve()
        scope = root / "scope"
        (scope / "keep").mkdir(parents=True)
        (scope / "keep" / "small.bin").write_bytes(b"k" * 100)
        victim = scope / "trash-me"
        victim.mkdir()
        (victim / "data.bin").write_bytes(os.urandom(65_536))
        environment = {key: value for key, value in os.environ.items() if key != "SPACE16X_READ_ONLY"}
        environment["XDG_DATA_HOME"] = str(root / "data")

        for command, env in (
            ([binary, "--read-only", scope], environment),
            ([binary, scope], {**environment, "SPACE16X_READ_ONLY": "1"}),
        ):
            with Terminal(command, env=env) as terminal:
                terminal.expect(b"trash-me")
                terminal.press(b" ", b"Read-only mode: Move to Trash is off.")
                terminal.press(b"d", b"Read-only mode: Move to Trash is off.")
                terminal.send(b"q")
                terminal.finish(0)
            assert victim.exists()

        with Terminal([binary, scope], env=environment) as terminal:
            terminal.expect(b"Scan complete")
            terminal.expect(b"trash-me")
            terminal.press(b" ", b"1 marked (")
            terminal.press(b"d", "Move “trash-me” (".encode())
            terminal.expect(b"to the Trash? y/N")
            terminal.press(b"n", b"Nothing was moved.")
            assert victim.exists()
            if sys.platform != "darwin" or os.environ.get("CI") == "true":
                terminal.press(b"d", b"to the Trash? y/N")
                terminal.press(b"y", b"Space is freed when you empty the Trash.")
                assert not victim.exists()
                terminal.press(b"j", b"keep")
                if sys.platform != "darwin":
                    trash = root / "data" / "Trash"
                    assert (trash / "files" / "trash-me" / "data.bin").is_file()
                    info = (trash / "info" / "trash-me.trashinfo").read_text()
                    assert f"\nPath={victim}\n" in info, info
            terminal.send(b"q")
            terminal.finish(0)


def check_rescan(binary):
    """Rescan the open folder after it changes on disk, and the whole scope from the top."""
    with tempfile.TemporaryDirectory(prefix="space16x-rescan-") as directory:
        scope = Path(directory).resolve()
        grow = scope / "grow"
        grow.mkdir()
        (grow / "seed.bin").write_bytes(b"s" * 100)
        (scope / "steady.bin").write_bytes(b"t" * 100)
        with Terminal([binary, scope]) as terminal:
            terminal.expect(b"Scan complete")
            terminal.expect(b"grow")
            terminal.press(b"\r", b"seed.bin")
            (grow / "added.bin").write_bytes(os.urandom(200_000))
            terminal.press(b"r", "Rescanned “grow”: ".encode())
            terminal.expect(b"added.bin")
            terminal.press(b"\x7f", b"steady.bin")
            (scope / "later.bin").write_bytes(os.urandom(50_000))
            terminal.press(b"r", "Rescanned “".encode())
            terminal.expect(b"later.bin")
            terminal.send(b"q")
            terminal.finish(0)


def main():
    if len(sys.argv) != 2:
        raise SystemExit(f"Usage: {sys.argv[0]} PATH-TO-SPACE16X")
    binary = Path(sys.argv[1]).resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix="space16x-tui-") as directory:
        fixture = Path(directory).resolve()
        nested = fixture / "nested-alpha"
        nested.mkdir()
        (nested / "payload.bin").write_bytes(os.urandom(65_536))
        check(binary, fixture)
        check_sort_size_and_color(binary, fixture)
    check_trash(binary)
    check_rescan(binary)
    print("Terminal navigation, sorting, color, Trash, rescan, resize, and restoration checks passed.")


if __name__ == "__main__":
    main()
