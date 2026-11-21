# Security policy

## Supported versions

Security fixes go into the latest published release of the CLI and the Mac app. Older releases do not receive backported fixes, so upgrade to the newest version first and check whether the problem still occurs.

## Report a vulnerability

Report vulnerabilities privately. Open the repository's **Security** tab and choose **Report a vulnerability** if available. If the button is unavailable, use the contact email listed on the 16X Brains site to request a private reporting channel before sharing vulnerability details. [16X Brains](https://16xbrains.com/)

Include:

- the affected component: CLI, terminal explorer, Mac app, installer, or release workflow
- the version (`space16x --version`, or the app's About window), your OS, and the filesystem
- reproduction steps, using a synthetic fixture where possible
- what you expected to happen, and what an attacker gains

Leave out personal paths, file names, and credentials.

Use the private report for follow-up. Let the maintainer know if you prefer to stay anonymous in the release notes.

## Scope

Space 16X scans filesystem metadata only. Its scanners never open file contents or write, and it never contacts the network. Only Space 16X Pro's duplicate finder reads file contents, when the user runs `space16x duplicates` or clicks **Find Duplicates**, to compare files of the same length. Its one change to your files is Move to Trash: after the user confirms, it moves the chosen items into the Trash on their own disk. It never copies them to another disk or deletes them permanently. Apart from that, it writes only an export or HTML report file the user names and, in Space 16X Pro, a scan the user saves to the history: `space16x history save` writes to `$XDG_STATE_HOME/space16x/history` (by default `~/.local/state/space16x/history`), and the Mac app's Save to History to `~/Library/Application Support/Space 16X/History`, readable only by that user. In-scope reports include:

- scanner behavior that follows symlinks, crosses devices, or opens files contrary to [the CLI contract](docs/cli.md)
- Move to Trash behavior that acts on items the user didn't confirm, deletes or copies instead of moving, follows a symbolic link to its target, or uses a Trash folder that another user controls
- terminal or UI output that a crafted filename can use to spoof results or inject terminal escape sequences
- importing a crafted scan file that crashes Space 16X, reads or changes local files, or offers to trash anything
- installer behavior that writes outside its managed paths or accepts an unverified executable
- weaknesses in the release, signing, or provenance workflows

## Verify a download

Each CLI release archive ships with a `.sha256` file. Releases from 0.2.2 also provide GitHub build-provenance attestations. [Verify a release archive](docs/install.md#verify-a-release-archive)
