# Security policy

## Supported versions

Security fixes go into the latest published release of the CLI and the Mac app. Older releases do not receive backported fixes, so upgrade to the newest version first and check whether the problem still occurs.

## Report a vulnerability

Report vulnerabilities privately. Do not open a public issue. Use GitHub's private vulnerability reporting: open the repository's **Security** tab and choose **Report a vulnerability**.

Include:

- the affected component: CLI, terminal explorer, Mac app, installer, or release workflow
- the version (`space16x --version`, or the app's About window), your OS, and the filesystem
- reproduction steps, using a synthetic fixture where possible
- what you expected to happen, and what an attacker gains

Leave out personal paths, file names, and credentials.

The maintainer acknowledges reports within 5 business days. A fix or mitigation plan follows within 30 days for confirmed issues. You'll be credited in the release notes unless you'd rather stay anonymous.

## Scope

Space 16X reads filesystem metadata only. It never opens file contents, writes to the scanned tree, or contacts the network. In-scope reports include:

- scanner behavior that follows symlinks, crosses devices, or opens files contrary to [the CLI contract](docs/cli.md)
- terminal or UI output that a crafted filename can use to spoof results or inject terminal escape sequences
- installer behavior that writes outside its managed paths or accepts an unverified executable
- weaknesses in the release, signing, or provenance workflows

## Verify a download

Each CLI release archive ships with a `.sha256` file and a GitHub build-provenance attestation. [docs/install.md](docs/install.md#verify-a-release-archive) explains how to check both.
