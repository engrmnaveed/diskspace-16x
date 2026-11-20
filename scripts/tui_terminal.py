"""Drive the Space 16X explorer through a pseudo-terminal."""

import errno
import fcntl
import os
import re
import select
import signal
import struct
import subprocess
import termios
import time


CSI = re.compile(rb"\x1b\[[0-?]*[ -/]*[@-~]")
ENTER_SCREEN = b"\x1b[?1049h"
LEAVE_SCREEN = b"\x1b[?1049l"


class Terminal:
    def __init__(self, command, *, cwd=None, columns=100, rows=28, env=None):
        self.master, self.slave = os.openpty()
        self.process = None
        self.initial_attributes = termios.tcgetattr(self.slave)
        self.output = bytearray()
        try:
            self.resize(columns, rows)
            environment = {**(os.environ if env is None else env), "TERM": "xterm-256color"}
            self.process = subprocess.Popen(
                command,
                cwd=cwd,
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
            return b""
        try:
            chunk = os.read(self.master, 65536)
            self.output.extend(chunk)
            return chunk
        except OSError as error:
            if error.errno != errno.EIO:
                raise
            return b""

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
        # Keep reading while the explorer exits. A full pseudo-terminal buffer, which is
        # small on macOS, would otherwise block its last writes and the exit.
        deadline = time.monotonic() + 8
        while self.process.poll() is None and time.monotonic() < deadline:
            self.read(0.10)
        assert self.process.wait(timeout=1) == status
        deadline = time.monotonic() + 2
        while LEAVE_SCREEN not in self.output and time.monotonic() < deadline:
            self.read(0.10)
        assert ENTER_SCREEN in self.output and LEAVE_SCREEN in self.output
        assert termios.tcgetattr(self.slave) == self.initial_attributes
