# Promotional video

The Space 16X promo is a 29.5-second vertical Short. It opens with “What’s eating your disk space?”, launches the interactive terminal explorer by typing `space16x`, and shows folder navigation, file ranking, a one-shot report, and the Mac app. The README uses a six-second loop with the terminal explorer and desktop side by side.

## Build locally

On macOS, use Swift 6 or later, Python 3.10 or later, Rustup, and FFmpeg with `libx264`. Install FFmpeg with `brew install ffmpeg` if needed. From the repository root:

```sh
python3 -m venv .build/promo-venv
.build/promo-venv/bin/python -m pip install -r scripts/promo/requirements.txt
.build/promo-venv/bin/python scripts/build-promo.py
```

For subsequent builds, run just the last command. The script builds the Rust CLI with the pinned toolchain and starts it with no arguments in a pseudo-terminal. It renders the real terminal screens with [pyte](https://pypi.org/project/pyte/0.8.2/), records a plain-text CLI report, and captures the shipping SwiftUI views in an offscreen window. Both scanners read the same synthetic folder. FFmpeg adds the motion, cuts, and original synthesized music. Once dependencies are installed, rendering runs offline and needs no screen-recording permissions.

| Output | Use |
| --- | --- |
| `dist/promo/space16x-short.mp4` | YouTube upload at 1080 × 1920, 30 fps, H.264/AAC |
| `dist/promo/space16x-preview.gif` | Looping GitHub README preview at 900 × 500 |

Pass `--cli path/to/space16x` to reuse a built binary or `--output path/to/folder` to place the exports elsewhere.

| Time | On screen |
| --- | --- |
| 0–2.5 s | Disk-space question and a preview of both interfaces |
| 2.5–7 s | Type `space16x` and see the interactive folder list |
| 7–10 s | Press Enter to open Exports |
| 10–13.5 s | Press `f` to rank the largest files |
| 13.5–16.5 s | Print a one-shot report with `--top 5` |
| 16.5–23 s | Mac app treemap and donut chart |
| 23–25.5 s | Mac app Largest Files view |
| 25.5–29.5 s | Free and open-source call to action with the GitHub handle |

## Refresh for a major change

[AGENTS.md](../AGENTS.md) requires a promo refresh for major UI/UX changes and major user-facing functionality. Show the new behavior using real output. Update the terminal capture in [`scripts/promo/terminal_capture.py`](../scripts/promo/terminal_capture.py), the desktop capture in [`scripts/promo/Capture.swift`](../scripts/promo/Capture.swift), or the scenes in [`scripts/promo/Cards.swift`](../scripts/promo/Cards.swift) as applicable. Change timing or fixture data in [`scripts/build-promo.py`](../scripts/build-promo.py).

The [Promo video workflow](../.github/workflows/promo.yml) renders changes to the promo sources on pull requests and on pushes to `main`. It can also be run manually. It attaches the MP4 and GIF to the run for 30 days. After a successful render on `main`, the workflow mentions the repository owner in a commit comment with the download link and a prompt to publish. Locally, the agent hands off the generated MP4 and shows the updated GIF in the README.

## Publish or refresh

1. Watch the Short at phone size and check the command, chart labels, call to action, and soundtrack.
2. In **YouTube Studio → Customization → Profile → Links**, add **Space 16X on GitHub** pointing to `https://github.com/engrmnaveed/diskspace-16x`. The channel profile link is clickable. The same URL in a Shorts description or comment appears as text. [YouTube link rules](https://support.google.com/youtube/answer/13748639) · [Set up profile links](https://support.google.com/youtubecreatorstudio/answer/2657964)
3. Upload `space16x-short.mp4` in YouTube Studio. [YouTube upload guide](https://support.google.com/youtube/answer/12779649)
4. Copy the regenerated GIF to `Branding/Space16X-Demo.gif`. Once the Short is live, wrap the README’s Markdown image in a link to the Short. [GitHub Markdown images and links](https://docs.github.com/en/get-started/writing-on-github/getting-started-with-writing-and-formatting-on-github/basic-writing-and-formatting-syntax#images)

**Suggested title**  What’s eating your disk space? | Space 16X

**Suggested description**  Find the files and folders taking up space. Space 16X is a free, open-source disk analyzer with a native Mac app and an interactive CLI for macOS and Linux. Get it through the GitHub link on our profile.

## Capture options for future clips

Use OBS Studio’s single-window capture for desktop clips that need to show a live gesture such as drag-and-drop. FFmpeg can incorporate the footage into the same edit. [OBS macOS Screen Capture](https://obsproject.com/kb/macos-screen-capture-source) · [FFmpeg filters](https://ffmpeg.org/ffmpeg-filters.html)
