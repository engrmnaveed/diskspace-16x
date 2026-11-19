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

Space 16X reads filesystem metadata only. It never opens file contents, writes to the scanned tree, or contacts the network. In-scope reports include:

- scanner behavior that follows symlinks, crosses devices, or opens files contrary to [the CLI contract](docs/cli.md)
- terminal or UI output that a crafted filename can use to spoof results or inject terminal escape sequences
- installer behavior that writes outside its managed paths or accepts an unverified executable
- weaknesses in the release, signing, or provenance workflows

## Verify a download

Each CLI release archive ships with a `.sha256` file. Releases from 0.2.2 also provide GitHub build-provenance attestations. [Verify a release archive](docs/install.md#verify-a-release-archive)
