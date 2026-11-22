# Promotional video

The Space 16X promo is a 29-second vertical Short. It opens with “What’s eating your disk space?”, launches the interactive terminal explorer by typing `space16x`, and shows folder navigation, file ranking, Move to Trash, Pro's cleanup suggestions with an Xcode archive to review, a server's scan piped into the explorer, the Mac app's charts, and Pro's clone-aware duplicate finder. The README uses a six-second loop with the terminal explorer and desktop side by side.

## Build locally

On macOS, use Swift 6 or later, Python 3.10 or later, Rustup, and FFmpeg with `libx264`. Install FFmpeg with `brew install ffmpeg` if needed. From the repository root:

```sh
python3 -m venv .build/promo-venv
.build/promo-venv/bin/python -m pip install -r scripts/promo/requirements.txt
.build/promo-venv/bin/python scripts/build-promo.py
```

For subsequent builds, run just the last command. The script builds the Pro edition of the Rust CLI with the pinned toolchain and starts it with no arguments in a pseudo-terminal. It renders the real terminal screens with [pyte](https://pypi.org/project/pyte/0.8.2/), records a plain-text CLI report, and captures the shipping SwiftUI views in an offscreen window. Both scanners read the same synthetic folder. The import scene pipes that folder's export, with its root renamed to `/srv`, into `space16x --import -`. The folder holds a Cargo `target` folder and a `node_modules` folder, which the cleanup report lists, an Xcode archive, which it lists to review, and one video saved twice plus an APFS clone of it (`cp -c`), which the Duplicates scene finds after clicking **Find Duplicates** and marks as a clone that frees nothing. The fixture's files get modification dates from today back to last year, which the Modified column shows. FFmpeg adds the motion, cuts, and original synthesized music. Once dependencies are installed, rendering runs offline and needs no screen-recording permissions.

| Output | Use |
| --- | --- |
| `dist/promo/space16x-short.mp4` | YouTube upload at 1080 × 1920, 30 fps, H.264/AAC |
| `dist/promo/space16x-preview.gif` | Looping GitHub README preview at 900 × 500 |

Pass `--cli path/to/space16x` to reuse a built binary or `--output path/to/folder` to place the exports elsewhere.

| Time | On screen |
| --- | --- |
| 0–2.5 s | Disk-space question and a preview of both interfaces |
| 2.5–6 s | Type `space16x` and see the interactive folder list |
| 6–8 s | Press Enter to open Exports |
| 8–10.5 s | Press `f` to rank the largest files |
| 10.5–13 s | Mark two files with Space and press `d` for the Move to Trash prompt |
| 13–15 s | Pro's cleanup suggestions with `scan --cleanup`: Rust build output and Node packages, then an Xcode archive to review |
| 15–18 s | Pipe a server's saved scan from `ssh` into `space16x --import -`, which opens it read-only |
| 18–23 s | Mac app treemap and donut chart |
| 23–26 s | Pro's Duplicates: a video saved twice and a clone of it, marked "Clone, frees nothing", after **Find Duplicates** |
| 26–29 s | Free and open-source call to action with the GitHub handle |

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

## User guide media

[`scripts/build-guide-media.py`](../scripts/build-guide-media.py) builds the [user guide](guide/README.md)'s screenshots and GIFs, and its two walkthrough videos. Like the promo, it renders the shipping SwiftUI views offscreen and runs the real Basic and Pro CLI in a pseudo-terminal, all on one synthetic home folder, so it needs no screen-recording permission and shows no real paths. With the promo's virtual environment set up as above:

```sh
.build/promo-venv/bin/python scripts/build-guide-media.py
```

It builds both CLI editions into `cli/target/guide-basic` and `cli/target/guide-pro`, or reuses binaries passed with `--cli` and `--cli-pro`. `--no-videos` skips the videos.

| Output | Use |
| --- | --- |
| `docs/guide/images/*.png` | Screenshots, about 1,040 pixels wide, in the light appearance. Commit them. |
| `docs/guide/images/*.gif` | Short loops beside the guide's sections, each well under 1 MB. Commit them. |
| `dist/guide/space16x-mac-walkthrough.mp4`, `dist/guide/space16x-terminal-walkthrough.mp4` | Walkthroughs at 1920 × 1080, 30 fps, H.264/AAC, with captions and quiet synthesized music. Upload them to YouTube. |
| `dist/guide/space16x-*-youtube.txt` | Each video's title, description, chapter timestamps and tags. |

The pieces live in [`scripts/guide/`](../scripts/guide/): `fixture.py` makes the folder, `terminal.py` and `storyboard.py` capture the explorer and lay out every screenshot, GIF and video scene, `GuideCapture.swift` drives the app's views, and `GuideRender.swift` draws the windows, captions and terminal screens. When the UI changes, rerun the script and commit the new images. Change what a scene shows in `storyboard.py`. The script stops with a clear message when an expected screen doesn't appear or a YouTube chapter would last less than 10 seconds. These files are outside the promo workflow's paths, so changing them doesn't render a new Short.

To publish a walkthrough, upload its MP4 in YouTube Studio with the title, description and chapters from its text file. Then replace "coming soon" in `docs/guide/README.md` and the matching quick start with the link. Each spot has an HTML comment that says what to write.
