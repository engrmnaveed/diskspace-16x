#!/usr/bin/env python3
"""Exercise the packaged explorer through a pseudo-terminal."""

import errno
import fcntl
import os
from pathlib import Path
import re
import select
import signal
import struct
import subprocess
import sys
import tempfile
import termios
import time
import tty


CSI = re.compile(rb"\x1b\[[0-?]*[ -/]*[@-~]")
ENTER_SCREEN = b"\x1b[?1049h"
LEAVE_SCREEN = b"\x1b[?1049l"


class Terminal:
    def __init__(self, binary, fixture):
        self.master, self.slave = os.openpty()
        self.process = None
        self.initial_attributes = termios.tcgetattr(self.slave)
        self.output = bytearray()
        try:
            self.resize(100, 28)
            environment = {**os.environ, "TERM": "xterm-256color"}
            self.process = subprocess.Popen(
                [binary, fixture],
                stdin=self.slave,
                stdout=self.slave,
                stderr=self.slave,
                env=environment,
            )
        except BaseException:
            os.close(self.master)
            os.close(self.slave)
            raise

    def __enter__(self):
        return self

    def __exit__(self, *_):
        if self.process.poll() is None:
            self.process.kill()
            self.process.wait()
        os.close(self.master)
        os.close(self.slave)

    def resize(self, columns, rows):
        dimensions = struct.pack("HHHH", rows, columns, 0, 0)
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ, dimensions)
        if self.process is not None:
            self.process.send_signal(signal.SIGWINCH)

    def send(self, keys):
        os.write(self.master, keys)

    def read(self, timeout):
        if not select.select([self.master], [], [], timeout)[0]:
            return
        try:
            self.output.extend(os.read(self.master, 65536))
        except OSError as error:
            if error.errno != errno.EIO:
                raise

    def expect(self, needle, start=0):
        deadline = time.monotonic() + 8
        while time.monotonic() < deadline:
            if needle in CSI.sub(b"", self.output[start:]):
                return
            if self.process.poll() is not None:
                break
            self.read(0.10)
        visible = CSI.sub(b"", self.output[-1800:]).decode(errors="replace")
        raise AssertionError(f"Expected {needle!r} in the terminal. Tail: {visible!r}")

    def finish(self, status):
        assert self.process.wait(timeout=8) == status
        deadline = time.monotonic() + 2
        while LEAVE_SCREEN not in self.output and time.monotonic() < deadline:
            self.read(0.10)
        assert ENTER_SCREEN in self.output and LEAVE_SCREEN in self.output
        assert termios.tcgetattr(self.slave) == self.initial_attributes


def check(binary, fixture):
    with Terminal(binary, fixture) as terminal:
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

    with Terminal(binary, fixture) as terminal:
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
