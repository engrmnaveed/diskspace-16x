"""Capture the real explorer's VT screen after each scripted keypress."""

from itertools import groupby
import os
import time

import pyte

from tui_terminal import Terminal


COLUMNS = 52
ROWS = 13
PALETTE = {
    "black": 0x08131E,
    "red": 0xF0848D,
    "green": 0x91DE88,
    "brown": 0xF7C16E,
    "blue": 0x87B8FF,
    "magenta": 0xE59BFA,
    "cyan": 0x72F6DC,
    "white": 0xF2FAFF,
    "brightblack": 0x849BAC,
}


def color(value, default):
    if value == "default":
        return default
    if value in PALETTE:
        return PALETTE[value]
    if value.removeprefix("bright") in PALETTE:
        return PALETTE[value.removeprefix("bright")]
    return int(value, 16)


def style(cell):
    fg = color(cell.fg, 0xC2D8E5)
    bg = color(cell.bg, None)
    if cell.reverse:
        return bg if bg is not None else 0x0A1724, fg, cell.bold
    return fg, bg, cell.bold


def snapshot(screen, fixture):
    rows = []
    for index, line in enumerate(screen.display):
        # Use the fixture name in place of its random on-disk prefix.
        if line.startswith(("Scope  ", "Selected  ")):
            label, path = line.split("  ", 1)
            path = path.strip()
            if fixture.name in path:
                path = path[path.index(fixture.name):]
                rows.append([
                    dict(column=0, columns=len(label), text=label, foreground=0x849BAC, bold=False),
                    dict(column=len(label) + 2, columns=len(path), text=path, foreground=0xC2D8E5, bold=False),
                ])
                continue

        spans = []
        column = 0
        for (foreground, background, bold), group in groupby(
            (screen.buffer[index][column] for column in range(COLUMNS)), key=style,
        ):
            cells = list(group)
            text = "".join(cell.data for cell in cells).rstrip()
            if text or background is not None:
                spans.append(dict(column=column, columns=len(cells), text=text,
                                  foreground=foreground, background=background, bold=bold))
            column += len(cells)
        rows.append(spans)
    return rows


def wait_for(terminal, stream, screen, *markers):
    deadline = time.monotonic() + 8
    while time.monotonic() < deadline:
        chunk = terminal.read(0.10)
        if chunk:
            stream.feed(chunk)
        elif terminal.process.poll() is not None:
            break
        else:
            visible = "\n".join(screen.display)
            if all(marker in visible for marker in markers):
                return
    visible = "\n".join(screen.display)
    raise RuntimeError(f"Expected {markers!r} in Space 16X explorer. Visible screen:\n{visible}")


def capture_explorer(binary, fixture):
    screen = pyte.Screen(COLUMNS, ROWS)
    stream = pyte.ByteStream(screen)
    frames = {}
    environment = os.environ.copy()
    environment.pop("NO_COLOR", None)
    # Running the executable with no arguments exercises the default htop-style explorer.
    with Terminal([binary], cwd=fixture, columns=COLUMNS, rows=ROWS, env=environment) as terminal:
        wait_for(terminal, stream, screen, "Scan complete", "Exports", "Media", "4 items")
        frames["folders"] = snapshot(screen, fixture)

        terminal.send(b"\r")
        wait_for(terminal, stream, screen, "launch.mov", "teaser.mp4", "2 items")
        frames["drill"] = snapshot(screen, fixture)

        terminal.send(b"\x7f")
        wait_for(terminal, stream, screen, "Exports", "Media", "4 items")
        terminal.send(b"f")
        wait_for(terminal, stream, screen, "Largest files", "55 shown", "interview.wav")
        frames["rank"] = snapshot(screen, fixture)

        # Mark the two largest files and ask to move them. Answer n, so nothing moves.
        terminal.send(b" ")
        wait_for(terminal, stream, screen, "1 marked")
        terminal.send(b" ")
        wait_for(terminal, stream, screen, "2 marked")
        terminal.send(b"d")
        wait_for(terminal, stream, screen, "Trash? y/N")
        frames["trash"] = snapshot(screen, fixture)
        terminal.send(b"n")
        wait_for(terminal, stream, screen, "Nothing was moved.")

        terminal.send(b"q")
        terminal.finish(0)
    return dict(columns=COLUMNS, frames=frames)
