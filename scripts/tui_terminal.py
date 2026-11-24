"""Drive the DiskUse16x explorer through a pseudo-terminal."""

import errno
import fcntl
import os
import re
import select
import signal
import struct
import subprocess
import sys
import termios
import time


CSI = re.compile(rb"\x1b\[[0-?]*[ -/]*[@-~]")
ENTER_SCREEN = b"\x1b[?1049h"
LEAVE_SCREEN = b"\x1b[?1049l"


class Terminal:
    def __init__(self, command, *, cwd=None, columns=100, rows=28, env=None, stdin=None):
        """With `stdin`, the explorer reads that file and takes keys from its terminal."""
        self.master, self.slave = os.openpty()
        self.process = None
        self.initial_attributes = termios.tcgetattr(self.slave)
        self.output = bytearray()
        self.columns, self.rows = columns, rows
        self.own_session = stdin is not None
        try:
            self.resize(columns, rows)
            environment = {**(os.environ if env is None else env), "TERM": "xterm-256color"}
            # A new session makes the pseudo-terminal its /dev/tty, as a shell would.
            controlling = None if stdin is None else (
                lambda: fcntl.ioctl(1, termios.TIOCSCTTY, 0)
            )
            self.process = subprocess.Popen(
                command,
                cwd=cwd,
                stdin=self.slave if stdin is None else stdin,
                stdout=self.slave,
                stderr=self.slave,
                env=environment,
                start_new_session=self.own_session,
                preexec_fn=controlling,
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
        self.columns, self.rows = columns, rows
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

    def settle(self, quiet=0.3, limit=3):
        """Reads until the explorer has been quiet for `quiet` seconds."""
        deadline = time.monotonic() + limit
        while time.monotonic() < deadline and self.read(quiet):
            pass

    def press(self, keys, needle):
        """Sends keys and waits for `needle`, forcing full repaints until it appears.

        The explorer redraws only changed cells, so a label that shares letters with the
        old one never appears whole, and a resize repaints every cell. crossterm drops a
        key that arrives in the same wakeup as SIGWINCH until the next key, so each key
        and each resize gets a quiet moment to be read and drawn before the next one.
        """
        start = len(self.output)
        self.send(keys)
        self.settle()
        widths = (self.columns + 1, self.columns)
        for attempt in range(8):
            if needle in CSI.sub(b"", self.output[start:]) or self.process.poll() is not None:
                break
            self.resize(widths[attempt % 2], self.rows)
            self.settle()
        if needle not in CSI.sub(b"", self.output[start:]):
            visible = CSI.sub(b"", self.output[-1800:]).decode(errors="replace")
            raise AssertionError(f"Expected {needle!r} after {keys!r}. Tail: {visible!r}")

    def finish(self, status):
        # Keep reading while the explorer exits. A full pseudo-terminal buffer, which is
        # small on macOS, would otherwise block its last writes and the exit.
        deadline = time.monotonic() + 8
        while self.process.poll() is None and time.monotonic() < deadline:
            self.read(0.10)
        if self.process.poll() is None:
            self.report_hang()
        assert self.process.wait(timeout=1) == status
        deadline = time.monotonic() + 2
        while LEAVE_SCREEN not in self.output and time.monotonic() < deadline:
            self.read(0.10)
        assert ENTER_SCREEN in self.output and LEAVE_SCREEN in self.output
        # macOS revokes a session's terminal when its leader exits, so only a terminal
        # shared with this process can be checked afterwards.
        if not self.own_session:
            assert termios.tcgetattr(self.slave) == self.initial_attributes

    def report_hang(self):
        """Prints where a process that should have exited is stuck, then fails."""
        visible = CSI.sub(b"", self.output[-600:]).decode(errors="replace")
        print(f"The explorer didn't exit. Tail: {visible!r}", file=sys.stderr)
        if sys.platform == "darwin":
            subprocess.run(["sample", str(self.process.pid), "2"], stdout=sys.stderr, check=False)
        raise AssertionError("The explorer didn't exit within 8 seconds.")
