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

        # Resizing repaints every cell, so the whole label appears in the output.
        start = len(terminal.output)
        terminal.send(b"s")
        time.sleep(0.3)
        terminal.resize(101, 28)
        terminal.expect(b"by items", start)

        start = len(terminal.output)
        terminal.send(b"a")
        time.sleep(0.3)
        terminal.resize(100, 28)
        terminal.expect(b"apparent", start)
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
    print("Terminal navigation, sorting, color, resize, and restoration checks passed.")


if __name__ == "__main__":
    main()
