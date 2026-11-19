#!/usr/bin/env python3
"""Exercise the packaged explorer through a pseudo-terminal."""

import os
from pathlib import Path
import sys
import tempfile
import termios
import tty

from tui_terminal import Terminal


def check(binary, fixture):
    with Terminal([binary, fixture]) as terminal:
        terminal.expect(b"nested-alpha")
        start = len(terminal.output)
        terminal.send(b"\r")
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
    print("Terminal navigation, resize, and restoration checks passed.")


if __name__ == "__main__":
    main()
