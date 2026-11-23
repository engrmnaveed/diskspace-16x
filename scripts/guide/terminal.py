"""Capture the real DiskUse16x explorer and reports for the user guide, in a light theme."""

import os
import subprocess
import time

import pyte

from tui_terminal import Terminal


COLUMNS = 100
ROWS = 24
FOREGROUND = 0x1F2A33
PROMPT = 0x086958
# The explorer's 16 ANSI colors, as pyte reports them, redrawn for a white window.
NAMES = {"black": "000000", "red": "cd0000", "green": "00cd00", "brown": "cdcd00", "yellow": "cdcd00",
         "blue": "0000ee", "magenta": "cd00cd", "cyan": "00cdcd", "white": "e5e5e5"}
INK = {"000000": 0x14212B, "cd0000": 0xB3261E, "00cd00": 0x2E7D32, "cdcd00": 0x8A5A00, "0000ee": 0x2457C5,
       "cd00cd": 0x8E3AA6, "00cdcd": 0x08776A, "e5e5e5": FOREGROUND, "ffffff": FOREGROUND, "7f7f7f": 0x6B7884}
FILL = {"00cdcd": 0xCBEDE7, "cdcd00": 0xF5E6B8, "cd0000": 0xF6D3D0, "00cd00": 0xD3EBD4, "0000ee": 0xD6E1F7,
        "cd00cd": 0xEBD7F2, "000000": 0x14212B, "e5e5e5": 0xEEF1F4}


def _hex(value):
    value = value.removeprefix("bright")
    return NAMES.get(value, value)


def _mix(value, other, amount):
    channels = [((value >> shift) & 255) * (1 - amount) + ((other >> shift) & 255) * amount for shift in (16, 8, 0)]
    return (round(channels[0]) << 16) | (round(channels[1]) << 8) | round(channels[2])


def _ink(value):
    if value == "default":
        return FOREGROUND
    key = _hex(value)
    if key in INK:
        return INK[key]
    try:
        return _mix(int(key, 16), 0x000000, 0.35)
    except ValueError:
        return FOREGROUND


def _fill(value):
    if value == "default":
        return None
    key = _hex(value)
    if key in FILL:
        return FILL[key]
    try:
        return _mix(int(key, 16), 0xFFFFFF, 0.75)
    except ValueError:
        return None


def _style(cell):
    fg, bg = _ink(cell.fg), _fill(cell.bg)
    if cell.reverse:
        return (0xFFFFFF if bg is None else _ink(cell.bg)), (fg if bg is None else _fill(cell.fg)), cell.bold
    return fg, bg, cell.bold


def _spans(cells):
    """Group (character, style) cells into spans of one style."""
    spans = []
    column = 0
    while column < len(cells):
        style = cells[column][1]
        end = column
        while end < len(cells) and cells[end][1] == style:
            end += 1
        text = "".join(character for character, _ in cells[column:end])
        if text.strip() or style[1] is not None:
            spans.append(dict(column=column, text=text.rstrip() if style[1] is None else text,
                              fg=style[0], bg=style[1], bold=style[2]))
        column = end
    return spans


def _shorten(cells, root):
    """Show the temporary folder as ~, keeping a box's right border in place."""
    plain = (FOREGROUND, None, False)
    while True:
        text = "".join(character for character, _ in cells)
        start = text.find(root)
        if start < 0:
            return cells
        style = cells[start][1]
        cells = cells[:start] + [("~", style)] + cells[start + len(root):]
        # Pad before a box's right border, or at the end of the line.
        end = next((index for index in range(start + 1, len(cells)) if cells[index][0] == "│"), len(cells))
        cells = (cells[:end] + [(" ", plain)] * (len(root) - 1) + cells[end:])[:COLUMNS]


def snapshot(screen, root):
    rows = []
    for index in range(screen.lines):
        cells = [(screen.buffer[index][column].data or " ", _style(screen.buffer[index][column]))
                 for column in range(screen.columns)]
        rows.append(_spans(_shorten(cells, root)))
    return rows


def text_frame(lines, root, command=None, cursor=False):
    """A shell prompt with a command, then output lines, as terminal rows."""
    rows = []
    if command is not None:
        row = [dict(column=0, text="$", fg=PROMPT, bg=None, bold=True)]
        if command:
            row.append(dict(column=2, text=command, fg=FOREGROUND, bg=None, bold=True))
        if cursor:
            row.append(dict(column=2 + len(command), text=" ", fg=FOREGROUND, bg=0x1F2A33, bold=False))
        rows.append(row)
    for line in lines:
        line = line.replace(root, "~")[:COLUMNS]
        rows.append([dict(column=0, text=line, fg=FOREGROUND, bg=None, bold=False)] if line.strip() else [])
    return rows


def typing(command, root, steps=None):
    """Frames that type a command one chunk at a time, ending with the whole command."""
    steps = steps or len(command)
    frames = []
    for index in range(1, steps + 1):
        frames.append(text_frame([], root, command[: round(len(command) * index / steps)], cursor=True))
    return frames


def report(binary, root, arguments, cwd=None, env=None):
    """The plain-text output of a command, with standard error where a terminal shows it."""
    result = subprocess.run([binary, *arguments], cwd=cwd, env=env, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    if result.returncode not in (0, 2):
        raise RuntimeError(f"diskuse16x {' '.join(map(str, arguments))} failed:\n{result.stdout}")
    return result.stdout.rstrip("\n").split("\n")


class Explorer:
    """One explorer session in a pseudo-terminal, recording named frames."""

    def __init__(self, command, root, frames, *, cwd=None, stdin=None, env=None):
        self.screen = pyte.Screen(COLUMNS, ROWS)
        self.stream = pyte.ByteStream(self.screen)
        self.root = root
        self.frames = frames
        environment = (env or os.environ).copy()
        environment.pop("NO_COLOR", None)
        self.terminal = Terminal(command, cwd=cwd, columns=COLUMNS, rows=ROWS, env=environment, stdin=stdin)

    def __enter__(self):
        return self

    def __exit__(self, *details):
        self.terminal.__exit__(*details)

    def _visible(self):
        return "\n".join(self.screen.display).replace(self.root, "~")

    def wait(self, *markers, absent=(), timeout=30):
        """Wait for markers to show, then for the screen to stop changing."""
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            chunk = self.terminal.read(0.1)
            if chunk:
                self.stream.feed(chunk)
                continue
            if self.terminal.process.poll() is not None:
                break
            visible = self._visible()
            if all(marker in visible for marker in markers) and not any(marker in visible for marker in absent):
                quiet = time.monotonic() + 0.3
                while time.monotonic() < quiet:
                    chunk = self.terminal.read(0.1)
                    if chunk:
                        self.stream.feed(chunk)
                        quiet = time.monotonic() + 0.3
                return
        raise RuntimeError(f"Expected {markers!r} in the DiskUse16x explorer. Visible screen:\n{self._visible()}")

    def press(self, keys, *markers, absent=()):
        self.terminal.send(keys)
        self.wait(*markers, absent=absent)

    def keep(self, name):
        self.frames[name] = snapshot(self.screen, self.root)

    def quit(self):
        self.terminal.send(b"q")
        self.terminal.finish(0)


DOWN, UP, ENTER, ESCAPE, BACKSPACE = b"\x1b[B", b"\x1b[A", b"\r", b"\x1b", b"\x7f"
