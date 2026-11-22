"""What the user guide shows: terminal sessions, screenshots, GIFs and the two walkthroughs."""

import json
import subprocess

import fixture
from terminal import BACKSPACE, DOWN, ENTER, ESCAPE, Explorer, report, text_frame, typing

# Terminal frames ----------------------------------------------------------------------------

COMMANDS = {
    "brew": "brew install --cask engrmnaveed/tap/space16x",
    "explore": "space16x ~",
    "version": "space16x --version",
    "top": "space16x scan ~ --top 5",
    "ssh": "ssh -C server space16x scan /srv --export - | space16x --import -",
    "cleanup": "space16x scan ~ --cleanup",
    "diff": "space16x diff last-week.json ~ --budget 500M",
}


def typing_count(key):
    """How many frames type a command."""
    return min(len(COMMANDS[key]), 14)


def capture_terminal(basic, pro, home, work):
    """Run the real explorer and reports on the guide's folder, and return named frames."""
    root = str(home)
    frames = {}
    for key, command in COMMANDS.items():
        for index, frame in enumerate(typing(command, root, steps=typing_count(key))):
            frames[f"type-{key}-{index}"] = frame

    with Explorer([basic, home], root, frames) as explorer:
        explorer.wait("Scan complete", "Documents", "8 items")
        explorer.keep("t-folders")
        explorer.press(DOWN, "Selected  ~/Downloads")
        explorer.keep("t-down")
        explorer.press(ENTER, "Installer.dmg", "4 items")
        explorer.keep("t-open")
        explorer.press(ENTER, "File details")
        explorer.keep("t-file")
        explorer.press(ESCAPE, "Installer.dmg", absent=("File details",))
        explorer.press(BACKSPACE, "8 items", "Selected  ~/Downloads")
        explorer.keep("t-back")
        explorer.press(b"s", "by items")
        explorer.keep("t-sort-items")
        explorer.press(b"s", "by name")
        explorer.press(b"s", "Allocated ↓", absent=("by name", "by items"))
        explorer.press(b"/", "Enter apply")
        explorer.press(b"d", "/ d")
        explorer.press(b"o", "2 matches")
        explorer.press(ENTER, "Filter do")
        explorer.keep("t-filter")
        explorer.press(ESCAPE, "8 items", absent=("Filter do",))
        explorer.press(b"?", "Help")
        explorer.keep("t-help")
        explorer.press(ESCAPE, "8 items", absent=("Help",))
        explorer.press(b"f", "Largest files")
        explorer.keep("t-largest")
        explorer.press(DOWN, "Selected  ~/Desktop/Wedding edit - review.mp4")
        explorer.press(DOWN, "Selected  ~/Downloads/Wedding edit (1).mp4")
        explorer.press(b" ", "1 marked")
        explorer.keep("t-mark1")
        explorer.press(DOWN, "Selected  ~/Downloads/Installer.dmg")
        explorer.press(b" ", "2 marked")
        explorer.keep("t-mark2")
        # Ask to move them, then answer n, so nothing moves.
        explorer.press(b"d", "Trash? y/N")
        explorer.keep("t-confirm")
        explorer.press(b"n", "Nothing was moved.")
        explorer.quit()

    added = None
    try:
        with Explorer([basic, home], root, frames) as explorer:
            explorer.wait("Scan complete", "Documents")
            explorer.press(DOWN, "Selected  ~/Downloads")
            explorer.press(ENTER, "Installer.dmg")
            added = fixture.add_rescan_file(home)
            explorer.press(b"r", "Rescanned")
            explorer.keep("t-rescan")
            explorer.quit()
    finally:
        if added is not None:
            added.unlink()

    # A server's scan, as `ssh server space16x scan /srv --export -` would send it.
    server = json.loads(subprocess.check_output([basic, "scan", home, "--export", "-"], stderr=subprocess.DEVNULL))
    server[3][0]["name"] = "/srv"
    saved = work / "server.json"
    saved.write_text(json.dumps(server), encoding="utf-8")
    with saved.open("rb") as scan, Explorer([basic, "--import", "-"], root, frames, stdin=scan) as explorer:
        explorer.wait("imported", "/srv", "Documents")
        explorer.keep("t-import")
        explorer.quit()

    with Explorer([pro, home], root, frames) as explorer:
        explorer.wait("Scan complete", "Modified", "Documents")
        explorer.keep("p-folders")
        explorer.press(b"u", "suggestions", "Rust buil")
        explorer.keep("p-cleanup")
        explorer.press(ESCAPE, "8 items", absent=("suggestions",))
        explorer.press(b"D", "Press Enter to compare")
        explorer.keep("p-dupes-start")
        explorer.press(ENTER, "2 groups", "brochure.pdf")
        explorer.keep("p-dupes")
        explorer.press(ESCAPE, "8 items", absent=("groups",))
        explorer.press(b"m", "┌Movies/")
        explorer.keep("p-treemap")
        explorer.press(b"m", "Allocated ↓")
        explorer.press(b"t", "File types", ".heic")
        explorer.keep("p-types")
        explorer.press(ESCAPE, "8 items", absent=("File types",))
        explorer.press(b"X", "GB exclusive", "Exclusive")
        explorer.keep("p-exclusive")
        explorer.quit()

    frames["c-version"] = text_frame(report(basic, root, ["--version"]), root, COMMANDS["version"])
    frames["c-top"] = text_frame(report(basic, root, ["scan", home, "--top", "5"]), root, COMMANDS["top"])
    frames["c-cleanup"] = text_frame(report(pro, root, ["scan", home, "--cleanup"]), root, COMMANDS["cleanup"])
    frames["c-diff"] = text_frame(
        report(pro, root, ["diff", "last-week.json", home, "--budget", "500M"], cwd=work), root, COMMANDS["diff"],
    )
    return frames


# Screenshots --------------------------------------------------------------------------------

SHOTS = {
    # Mac app, Basic
    "mac-start": "start",
    "mac-folders": "folders",
    "mac-open-folder": "movies",
    "mac-treemap": "treemap",
    "mac-donut": "donut",
    "mac-largest-files": "largest",
    "mac-move-to-trash": "largest-alert",
    "mac-scan-details": "details",
    "mac-settings": "settings",
    # Mac app, Pro
    "mac-pro-cleanup": "pro-cleanup",
    "mac-pro-duplicates": "pro-duplicates",
    "mac-pro-exclusive": "pro-exclusive",
    "mac-pro-types": "pro-types",
    "mac-pro-compare": "pro-compare",
    "mac-pro-receipt": "pro-receipt",
    "mac-pro-explain": "pro-explain",
}
TERMINAL_SHOTS = {
    "terminal-folders": "t-folders",
    "terminal-file-details": "t-file",
    "terminal-trash": "t-confirm",
    "terminal-filter": "t-filter",
    "terminal-rescan": "t-rescan",
    "terminal-import": "t-import",
    "terminal-scan-top": "c-top",
    "terminal-pro-treemap": "p-treemap",
    "terminal-pro-diff": "c-diff",
}
SHOT_WIDTH = 1040
# Narrow windows keep their own proportions on the page.
NARROW = {"mac-settings": 560}


def shots(folder):
    outputs = []
    for name, source in SHOTS.items():
        outputs.append(dict(kind="shot", path=str(folder / f"{name}.png"), width=NARROW.get(name, SHOT_WIDTH),
                            source=source))
    for name, frame in TERMINAL_SHOTS.items():
        outputs.append(dict(kind="shot", path=str(folder / f"{name}.png"), width=SHOT_WIDTH, frame=frame))
    return outputs


# GIFs ---------------------------------------------------------------------------------------

TYPE = 0.07  # seconds per typing frame


def typed(key, caption, hold=0.7):
    """Typing frames for a command, as (content, caption, keys, seconds) steps."""
    steps = []
    count = typing_count(key)
    for index in range(count):
        steps.append((("terminal", f"type-{key}-{index}"), caption, [], hold if index == count - 1 else TYPE))
    return steps


GIFS = {
    "mac-first-scan": lambda: [
        (("window", "start"), "Click a disk, or choose a folder", [], 2.4),
        (("window", "folders"), "The biggest items are at the top", [], 2.6),
        (("window", "movies"), "Double-click a folder to open it", [], 2.2),
        (("window", "treemap"), "Treemap: bigger boxes use more space", ["⌘", "3"], 2.6),
        (("window", "donut"), "Donut: each item's share of the folder", ["⌘", "4"], 2.6),
        (("window", "largest"), "Largest Files: big files from anywhere", ["⌘", "2"], 2.8),
    ],
    "terminal-first-scan": lambda: [
        *typed("explore", "Type space16x and a folder"),
        (("terminal", "t-folders"), "The biggest items are at the top", [], 2.4),
        (("terminal", "t-down"), "Move the highlight", ["↓"], 1.4),
        (("terminal", "t-open"), "Open a folder", ["Enter"], 2.0),
        (("terminal", "t-file"), "See a file's details", ["Enter"], 2.4),
        (("terminal", "t-back"), "Go back up", ["←"], 1.4),
        (("terminal", "t-largest"), "List the largest files", ["f"], 2.4),
        (("terminal", "t-mark2"), "Mark items", ["Space"], 2.0),
        (("terminal", "t-confirm"), "Move them to the Trash, after you confirm", ["d"], 3.0),
    ],
    "mac-clean-up": lambda: [
        (("window", "largest"), "Find what you don't need", [], 1.8),
        (("window", "largest-selected"), "⌘-click to select several", ["⌘", "click"], 2.2),
        (("window", "largest-alert"), "Confirm, and they move to the Trash", ["⌘", "⌫"], 3.2),
    ],
    "pro-tour": lambda: [
        (("window", "pro-cleanup"), "Cleanup: folders that tools recreate", [], 2.8),
        (("window", "pro-duplicates"), "Duplicates: files you saved twice", [], 2.8),
        (("window", "pro-compare"), "Compare: what grew since last week", [], 2.8),
        (("window", "pro-explain"), "Explain Disk Space: what a scan can't see", [], 3.0),
    ],
}
GIF_WIDTH = 960
GIF_MARGIN = 16
CAPTION = 64


def terminal_height(width, rows, columns=100):
    """The terminal window's height, as GuideRender.swift draws it."""
    scale = width / 1000
    inset = 16 * scale
    cell = (width - 2 * inset) / columns
    return 34 * scale + 2 * inset + rows * round(cell * 2.05)


def gif_frames(folder, terminal_rows=24):
    """Canvas outputs and (path, seconds) sequences for each GIF."""
    outputs = []
    sequences = {}
    inner = GIF_WIDTH - 2 * GIF_MARGIN
    for name, steps in GIFS.items():
        steps = steps()
        is_terminal = steps[-1][0][0] == "terminal"
        content = terminal_height(inner, terminal_rows) if is_terminal else round(inner * 2 / 3)
        height = round(GIF_MARGIN + content + CAPTION)
        sequence = []
        for index, ((kind, source), caption, keys, seconds) in enumerate(steps):
            path = folder / f"{name}-{index:03}.png"
            layer = (dict(type="terminal", frame=source, rows=terminal_rows) if kind == "terminal"
                     else dict(type="window", source=source))
            layer.update(x=GIF_MARGIN, y=GIF_MARGIN, w=inner, h=content)
            caption_y = GIF_MARGIN + content + (CAPTION - 28) / 2
            layers = [layer, dict(type="text", text=caption, x=GIF_MARGIN + 4, y=caption_y - 1, size=22,
                                  weight="semibold", color=0x1F2A33)]
            if keys:
                layers.append(dict(type="keys", items=keys, x=GIF_WIDTH - GIF_MARGIN, y=caption_y - 6,
                                   size=36, align="right"))
            outputs.append(dict(kind="canvas", path=str(path), width=GIF_WIDTH, height=height,
                                background="light", layers=layers))
            sequence.append((path, seconds))
        sequences[name] = sequence
    return outputs, sequences


# Walkthrough videos -------------------------------------------------------------------------

WIDTH, HEIGHT = 1920, 1080
WHITE, MUTED, MINT = 0xF2FAFF, 0xADC7D8, 0x72F6DC
REPO = "github.com/engrmnaveed/diskspace-16x"


def scene(content, title, detail, keys=(), seconds=6.5):
    return dict(content=content, title=title, detail=detail, keys=list(keys), seconds=seconds)


def typing_scenes(key, title, detail, hold=1.2):
    count = typing_count(key)
    return [scene(("terminal", f"type-{key}-{index}"), title, detail,
                  seconds=hold if index == count - 1 else TYPE) for index in range(count)]


def mac_video():
    return dict(
        name="mac",
        label="MAC APP",
        title="Space 16X for Mac",
        subtitle="Find what's using your disk space",
        chapters=[
            ("Install and open", "Free, offline, no account", [
                *typing_scenes("brew", "Install it", "With Homebrew, or download the app from GitHub. It's free."),
                scene(("window", "start"), "Approve it once",
                      "First launch: click Done, then Open Anyway in Privacy & Security."),
            ]),
            ("Scan and read the results", "Biggest first, in seconds", [
                scene(("window", "start"), "Pick what to scan",
                      "Click a disk under Volumes, choose a folder, or drop one on the window."),
                scene(("window", "folders"), "Biggest first",
                      "Allocated is the space used. Items counts what's inside. Share is its slice."),
                scene(("window", "movies"), "Open a folder", "Double-click it, or press ⌘↓. Press ⌘↑ to go back up.",
                      ["⌘", "↓"]),
                scene(("window", "details"), "Scan Details", "What was skipped, and how sizes are counted.",
                      ["⌘", "I"], 5.0),
            ]),
            ("See it as a picture", "Treemap and donut", [
                scene(("window", "treemap"), "Treemap", "Bigger boxes use more space. Click a folder's box to open it.",
                      ["⌘", "3"]),
                scene(("window", "donut"), "Donut", "Each slice is an item's share of the folder.", ["⌘", "4"]),
            ]),
            ("Largest files and cleanup", "Free space safely", [
                scene(("window", "largest"), "Largest Files",
                      "The 100 biggest files anywhere in the scan. Double-click to show one in Finder.", ["⌘", "2"]),
                scene(("window", "largest-selected"), "Select what you don't need",
                      "⌘-click to pick several. The bar at the bottom adds them up.", ["⌘", "click"], 5.0),
                scene(("window", "largest-alert"), "Move to Trash",
                      "Confirm, and the totals update. Space comes back when you empty the Trash.", ["⌘", "⌫"], 6.5),
            ]),
            ("Rescan and saved scans", "Keep results fresh", [
                scene(("window", "movies"), "Rescan a folder",
                      "Changed files elsewhere? Scan → Rescan Folder updates just this folder.", ["⌥", "⌘", "R"]),
                scene(("window", "start"), "Save and open scans",
                      "File → Export Scan… keeps a scan. Open a Saved Scan… browses it, read-only.",
                      ["⇧", "⌘", "E"]),
            ]),
            ("Settings and Pro", "Pro is free too", [
                scene(("window", "settings"), "Settings",
                      "Skip folders like node_modules, and turn on Pro features, for free.", ["⌘", ","], 6.5),
                scene(("window", "pro-folders"), "Pro adds more views",
                      "A Modified column, Types, Cleanup, Duplicates and Compare."),
            ]),
            ("Pro: find more to free", "Cleanup, duplicates and more", [
                scene(("window", "pro-types"), "File types", "Which kinds of files use the most space.", ["⌘", "5"], 5.0),
                scene(("window", "pro-cleanup"), "Cleanup suggestions",
                      "Caches and build output that tools recreate. Archives to review stay apart.", ["⌘", "6"]),
                scene(("window", "pro-duplicates-start"), "Duplicates",
                      "Nothing is read until you click Find Duplicates.", ["⌘", "7"], 4.0),
                scene(("window", "pro-duplicates"), "Files you saved twice",
                      "Clones that share space are marked, so the total is what you can really free."),
                scene(("window", "pro-exclusive"), "Exclusive size",
                      "View → Show Exclusive Size: what removing each item frees at least.", seconds=5.5),
            ]),
            ("Pro: compare and explain", "What grew, and why the disk is full", [
                scene(("window", "pro-compare"), "Compare",
                      "Save a scan to the history. Later, see what grew or shrank since.", ["⇧", "⌘", "K"]),
                scene(("window", "pro-receipt"), "Footprint receipts",
                      "Measure what an install or build added, with an optional budget.", seconds=5.5),
                scene(("window", "pro-explain"), "Explain Disk Space",
                      "Snapshots, purgeable space and system volumes that a scan can't see.", ["⇧", "⌘", "X"]),
                scene(("window", "settings"), "Menu bar and windows",
                      "Free space in the menu bar with low-space alerts, and File → New Window.", seconds=5.5),
            ]),
        ],
    )


def terminal_video():
    return dict(
        name="terminal",
        label="TERMINAL",
        title="Space 16X in the terminal",
        subtitle="Explore your disk from the keyboard, on a Mac or Linux",
        chapters=[
            ("Install", "One line, no admin password", [
                *typing_scenes("version", "Install it with one line",
                               "Paste the installer from the install guide. No admin password needed.", hold=0.4),
                scene(("terminal", "c-version"), "Install it with one line",
                      "Paste the installer from the install guide. No admin password needed.", seconds=4.5),
            ]),
            ("Explore a folder", "Biggest first", [
                *typing_scenes("explore", "Open the explorer", "Type space16x and a folder. ~ is your home folder."),
                scene(("terminal", "t-folders"), "Biggest first",
                      "The bar shows each item's share. Items counts what's inside."),
                scene(("terminal", "t-down"), "Move", "↑ and ↓ move the highlight.", ["↓"], 3.0),
                scene(("terminal", "t-open"), "Open a folder", "Enter opens the highlighted folder.", ["Enter"], 4.5),
                scene(("terminal", "t-file"), "See a file's details", "Both sizes, the exact bytes and the full path.",
                      ["Enter"], 5.0),
                scene(("terminal", "t-back"), "Go back", "← or Backspace goes up a level.", ["←"], 3.0),
            ]),
            ("Find the largest files", "Sort, filter and help", [
                scene(("terminal", "t-largest"), "Largest files",
                      "f ranks the biggest files in the whole scan. Enter jumps to a file's folder.", ["f"]),
                scene(("terminal", "t-sort-items"), "Sort", "s cycles between size, item count and name.", ["s"], 4.5),
                scene(("terminal", "t-filter"), "Filter", "Press /, type part of a name, then Enter. Esc clears it.",
                      ["/"], 5.0),
                scene(("terminal", "t-help"), "Every key, one press away", "? shows the full list.", ["?"], 4.5),
            ]),
            ("Free up space", "Mark, confirm, done", [
                scene(("terminal", "t-mark1"), "Mark items", "Space marks the highlighted item with a ✓.", ["Space"], 3.5),
                scene(("terminal", "t-mark2"), "Mark as many as you like", "The top line adds up what's marked.",
                      ["Space"], 3.5),
                scene(("terminal", "t-confirm"), "Move to the Trash",
                      "Press y to confirm. Space comes back when you empty the Trash.", ["d"]),
                scene(("terminal", "t-rescan"), "Rescan a folder",
                      "After changes elsewhere, r rescans just the open folder.", ["r"], 5.0),
            ]),
            ("Reports and servers", "Scan anywhere, browse here", [
                *typing_scenes("top", "Text reports", "space16x scan prints a report to paste or save.", hold=0.4),
                scene(("terminal", "c-top"), "Text reports", "--top lists the largest files. --json suits scripts.",
                      seconds=5.0),
                *typing_scenes("ssh", "Check a server", "Scan it over ssh, then browse the result on your computer.",
                               hold=0.6),
                scene(("terminal", "t-import"), "Check a server",
                      "The saved scan opens read-only, so nothing on the server changes."),
            ]),
            ("Pro: find more to free", "Free, with install.sh --pro", [
                scene(("terminal", "p-folders"), "Pro is free too",
                      "Install it with --pro. It adds a Modified column and more views."),
                scene(("terminal", "p-cleanup"), "Cleanup suggestions",
                      "u lists caches and build output that tools recreate.", ["u"]),
                scene(("terminal", "p-dupes-start"), "Duplicates", "D opens the view. Nothing is read until Enter.",
                      ["D"], 3.5),
                scene(("terminal", "p-dupes"), "Files you saved twice",
                      "Each group shows its copies and the space the extras use.", ["Enter"]),
                scene(("terminal", "p-treemap"), "Treemap", "m draws the folder as boxes sized by space.", ["m"], 5.0),
                scene(("terminal", "p-types"), "File types", "t lists the kinds of files that use the most space.",
                      ["t"], 4.5),
                scene(("terminal", "p-exclusive"), "Exclusive size",
                      "X shows what removing each item frees at least.", ["X"], 4.5),
            ]),
            ("Pro: compare scans", "What grew, with a budget", [
                *typing_scenes("diff", "Compare scans", "See what grew since a saved scan.", hold=0.4),
                scene(("terminal", "c-diff"), "Compare scans",
                      "--budget says whether growth stayed within it, and fails a build if not.", seconds=6.5),
            ]),
        ],
    )


CARD = 1.8
INTRO = 4.0
OUTRO = 6.0


def _content_layer(content):
    kind, source = content
    if kind == "terminal":
        return dict(type="terminal", frame=source, rows=24, x=110, y=200, w=1700, h=836)
    return dict(type="window", source=source, x=110, y=200, w=1700, h=836)


def video_frames(video, folder):
    """Canvas outputs, (path, seconds) steps and chapter start times for one walkthrough."""
    outputs, steps, chapters = [], [], []
    total = len(video["chapters"])
    elapsed = 0.0

    def add(name, layers, seconds):
        nonlocal elapsed
        path = folder / f"{video['name']}-{len(steps):03}-{name}.png"
        outputs.append(dict(kind="canvas", path=str(path), width=WIDTH, height=HEIGHT, background="brand", layers=layers))
        steps.append((path, seconds))
        elapsed += seconds

    add("intro", [
        dict(type="icon", x=WIDTH / 2 - 110, y=200, w=220),
        dict(type="text", text=video["title"], x=0, y=460, w=WIDTH, size=104, weight="heavy", color=WHITE, align="center"),
        dict(type="text", text=video["subtitle"], x=0, y=600, w=WIDTH, size=42, weight="medium", color=MUTED, align="center"),
        dict(type="text", text=f"{video['label']} WALKTHROUGH  ·  FREE AND OPEN SOURCE", x=0, y=690, w=WIDTH,
             size=26, weight="semibold", color=MINT, mono=True, align="center"),
    ], INTRO)
    for number, (title, subtitle, scenes) in enumerate(video["chapters"], start=1):
        chapters.append((0.0 if number == 1 else elapsed, title))
        progress = dict(type="progress", x=110, y=1052, w=WIDTH - 220, h=6, progress=number, total=total)
        add("card", [
            dict(type="text", text=f"{number:02}", x=0, y=360, w=WIDTH, size=40, weight="semibold", color=MINT,
                 mono=True, align="center"),
            dict(type="text", text=title, x=0, y=420, w=WIDTH, size=92, weight="heavy", color=WHITE, align="center"),
            dict(type="text", text=subtitle, x=0, y=548, w=WIDTH, size=36, weight="medium", color=MUTED, align="center"),
            progress,
        ], CARD)
        for item in scenes:
            layers = [
                dict(type="icon", x=110, y=34, w=40),
                dict(type="text", text=f"{video['label']}  ·  {number:02}  {title.upper()}", x=162, y=43, size=21,
                     weight="semibold", color=MINT, mono=True),
                dict(type="text", text=item["title"], x=107, y=80, size=48, weight="heavy", color=WHITE),
                dict(type="text", text=item["detail"], x=110, y=142, w=1320, size=27, weight="medium", color=MUTED),
                _content_layer(item["content"]),
                progress,
            ]
            if item["keys"]:
                layers.append(dict(type="keys", items=item["keys"], x=WIDTH - 110, y=70, size=62, align="right"))
            add("scene", layers, item["seconds"])
    add("outro", [
        dict(type="icon", x=WIDTH / 2 - 90, y=200, w=180),
        dict(type="text", text="Free and open source", x=0, y=420, w=WIDTH, size=84, weight="heavy", color=WHITE,
             align="center"),
        dict(type="text", text=REPO, x=0, y=548, w=WIDTH, size=42, weight="semibold", color=MINT, mono=True,
             align="center"),
        dict(type="text", text="Read the user guide in docs/guide for every step.", x=0, y=624, w=WIDTH, size=34,
             weight="medium", color=MUTED, align="center"),
    ], OUTRO)
    # YouTube shows chapters only when each one lasts at least 10 seconds.
    ends = [start for start, _ in chapters[1:]] + [elapsed]
    for (start, title), end in zip(chapters, ends):
        if end - start < 10:
            raise ValueError(f"The {video['name']} chapter “{title}” lasts {end - start:.1f} seconds. YouTube needs 10.")
    return outputs, steps, chapters


def timestamp(seconds):
    seconds = int(seconds)
    return f"{seconds // 60}:{seconds % 60:02}"


YOUTUBE = {
    "mac": dict(
        title="Space 16X for Mac: find what's using your disk space (walkthrough)",
        intro=("See what's filling your Mac's disk and free space safely with Space 16X, a free and open-source "
               "disk space analyzer. This walkthrough covers scanning a disk or folder, the list, treemap and donut "
               "views, Largest Files, Move to Trash, rescanning and saved scans, then the free Pro views: cleanup "
               "suggestions, duplicates, exclusive size, Compare, Footprint receipts and Explain Disk Space."),
        tags="disk space analyzer, Mac, macOS, free up disk space, storage, treemap, duplicate finder, open source",
    ),
    "terminal": dict(
        title="Space 16X in the terminal: a disk space explorer for macOS and Linux (walkthrough)",
        intro=("Explore your disk from the keyboard with Space 16X, a free and open-source disk space analyzer for "
               "macOS and Linux. This walkthrough covers installing it with one line, the interactive explorer, the "
               "largest files, sorting and filtering, Move to Trash, rescanning, text reports and checking a server "
               "over ssh, then the free Pro views: cleanup suggestions, duplicates, the treemap, file types, "
               "exclusive size and comparing scans with a budget."),
        tags="disk space analyzer, CLI, terminal, Linux, macOS, ncdu alternative, server, du, open source",
    ),
}


def youtube_text(name, chapters, duration):
    info = YOUTUBE[name]
    lines = [
        f"Title: {info['title']}", "",
        "Description:", "",
        info["intro"], "",
        "Captions are on screen, and there's no voice-over.", "",
        "Chapters:",
        *(f"{timestamp(start)} {title}" for start, title in chapters), "",
        "Get Space 16X: https://github.com/engrmnaveed/diskspace-16x",
        "User guide: https://github.com/engrmnaveed/diskspace-16x/blob/main/docs/guide/README.md",
        "Install: https://github.com/engrmnaveed/diskspace-16x/blob/main/docs/install.md", "",
        f"Tags: {info['tags']}", "",
        f"Length: {timestamp(duration)}",
    ]
    return "\n".join(lines) + "\n"


THUMBNAILS = {
    "mac": (["What's using", "your disk?"], "Mac app walkthrough", ("window", "treemap")),
    "terminal": (["Find big", "files fast"], "Terminal walkthrough", ("terminal", "t-folders")),
}


def thumbnail(name, folder):
    """A 1280 × 720 YouTube thumbnail: a short question beside the real interface."""
    lines, subtitle, (kind, source) = THUMBNAILS[name]
    content = (dict(type="terminal", frame=source, rows=24) if kind == "terminal" else dict(type="window", source=source))
    content.update(x=620, y=100, w=620, h=580)
    layers = [
        dict(type="icon", x=60, y=58, w=56),
        dict(type="text", text="SPACE 16X", x=130, y=70, size=30, weight="bold", color=MINT, mono=True),
        *(dict(type="text", text=line, x=56, y=190 + index * 86, size=72, weight="heavy", color=WHITE)
          for index, line in enumerate(lines)),
        dict(type="text", text=subtitle, x=60, y=380, size=32, weight="semibold", color=MUTED),
        dict(type="text", text="FREE AND OPEN SOURCE", x=60, y=436, size=22, weight="semibold", color=MINT, mono=True),
        content,
    ]
    return dict(kind="canvas", path=str(folder / f"{name}-thumbnail.png"), width=1280, height=720,
                background="brand", layers=layers)


def build(frames, work, videos):
    """The whole specification for GuideCapture, and what to encode afterwards."""
    outputs = shots(work / "shots")
    gif_outputs, gif_sequences = gif_frames(work / "gif")
    outputs += gif_outputs
    plans = {}
    if videos:
        for video in (mac_video(), terminal_video()):
            video_outputs, steps, chapters = video_frames(video, work / "video")
            outputs += video_outputs
            outputs.append(thumbnail(video["name"], work / "video"))
            plans[video["name"]] = dict(steps=steps, chapters=chapters, thumbnail=outputs[-1]["path"])
    spec = dict(terminal=dict(columns=100, frames=frames), outputs=outputs)
    return spec, gif_sequences, plans
