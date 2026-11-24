# Release runbook

## Review before publication

Confirm the product name, runtime coverage for the release matrix, distribution channels, and company-site placement with the project owner. Record the scan benchmark on named hardware. Exercise the [macOS manual checks](manual-qa.md), including the macOS 14 deployment floor, before claiming support. Hosted CI is configured for Apple silicon and Intel on macOS 15, plus the CLI on Ubuntu 24.04 x86-64 and ARM64.

Check that [scripts/release-version](../scripts/release-version), [cli/Cargo.toml](../cli/Cargo.toml), and the CLI's `--version` output agree. Update the [changelog](../CHANGELOG.md) and the [installation guide](install.md) package examples. The GitHub release notes are the version's changelog section followed by the standard text about the assets, which [scripts/release-notes](../scripts/release-notes) prints: run `scripts/release-notes X.Y.Z` to preview them.

## Build release artifacts in GitHub Actions

Pushing a new `vX.Y.Z` tag that matches `scripts/release-version` runs **Publish release** (`publish-cli.yml`). The workflow tests the four Rust targets, runs the Swift tests, and builds the universal Mac app with `scripts/package-macos.sh --universal` and `scripts/archive-macos.sh`. It uses the `release` environment for publication. Once the environment's protection rules pass, the workflow attests build provenance and publishes these assets to the tagged GitHub release:

- the checksummed CLI archives for both editions: `diskuse16x-<target>.tar.gz` (Basic) and `diskuse16x-pro-<target>.tar.gz` (Pro), built from the same tag for all four targets
- `diskuse16x-X.Y.Z-macos-universal.zip` and its `.sha256` file
- `.deb` and `.rpm` packages of both editions for the two Linux targets, with `.sha256` files: `diskuse16x_<arch>.deb`, `diskuse16x-pro_<arch>.deb`, `diskuse16x.<arch>.rpm` and `diskuse16x-pro.<arch>.rpm`. Each Linux CI job builds them from its archives with `scripts/package-linux.sh` and installs, switches and removes them with `scripts/check-linux-packages.py`, on the runner for `.deb` and in a Fedora container for `.rpm`.
- `diskuse16x.rb`, the Homebrew cask rendered by `scripts/render-cask` from the app ZIP's digest
- `du16x.rb` and `du16x-pro.rb`, the CLI formulas rendered by `scripts/render-formula` from the archives' digests. `scripts/check-formula.sh` installs and tests both from local archives in the macOS arm64 and Linux x86-64 CI jobs.
- `install.sh` and `install.sh.sha256`

The published Mac app is ad hoc signed and not notarized. Developer ID signing is optional and runs in a separate workflow, described below.

### Homebrew tap

The cask and the CLI formulas are published through the public tap repository `engrmnaveed/homebrew-tap`, so users run `brew install --cask engrmnaveed/tap/diskuse16x` for the app and `brew install engrmnaveed/tap/du16x` (or `du16x-pro`) for the CLI. The formulas' names keep `brew install diskuse16x` pointing at the cask alone. Homebrew's official cask repository doesn't accept apps that fail Gatekeeper checks. The templates are [packaging/homebrew/diskuse16x.rb.in](../packaging/homebrew/diskuse16x.rb.in) and [packaging/homebrew/du16x.rb.in](../packaging/homebrew/du16x.rb.in).

- **One-time setup:** create `engrmnaveed/homebrew-tap` with a `Casks` folder. Generate an Ed25519 SSH key pair. Add the public key to the tap as a deploy key with write access. Store the private key as the `HOMEBREW_TAP_DEPLOY_KEY` secret in the `release` environment.
- **Each release:** the publish job commits `Casks/diskuse16x.rb`, `Formula/du16x.rb` and `Formula/du16x-pro.rb` to the tap after creating the release. Releases before 1.0 published the cask as `space16x`, so the job also removes `Casks/space16x.rb` and keeps `cask_renames.json` mapping it to `diskuse16x`, which moves its users over on `brew upgrade`.
- **Without the secret:** the job skips the tap update and logs a notice. Download the three `.rb` files from the release and commit them to the same paths in the tap by hand.

The install and uninstall one-liners in the README and the installation guide run `releases/latest/download/install.sh`, so they need no change when the installer does. The repository was named `engrmnaveed/diskspace-16x` before 1.0. GitHub redirects that name, but releases before 1.0 are attested under it.

### crates.io

The last step of **Publish release** runs `cargo publish --locked` from `cli/` when the `release` environment has a `CARGO_REGISTRY_TOKEN` secret. Without the secret, it logs a notice and skips. CI runs `cargo package --locked` in the Linux x86-64 job, so a release can't fail to package.

- **One-time setup:** sign in to [crates.io](https://crates.io/) with GitHub and verify an email address. Under **Account Settings → API Tokens**, create a token with the **publish-new** and **publish-update** scopes, limited to the crate `diskuse16x`. Store it as the `CARGO_REGISTRY_TOKEN` secret in the `release` environment. After the first publish, you can narrow the token to **publish-update**, or switch to crates.io's trusted publishing.
- **Each release:** the step publishes the tagged version once, and a rerun skips a version crates.io already has. crates.io keeps every version for good: a broken one can only be yanked.

### One-time repository settings

These settings live on GitHub, not in the repository. Set them before the next tag:

- **Environments → `release`:** add yourself, or the release owners, as required reviewers. Restrict deployments to tags matching `v*`. Without reviewers, a tag push publishes immediately.
- **Rules → Rulesets:** protect tags matching `v*` from creation, update, and deletion by anyone outside the release owners. Protect `main` with required pull requests and the CI status checks.
- **Settings → General → Releases:** enable immutable releases, so published assets and their tags can't be replaced.
- **Security → Private vulnerability reporting** Enable it to accept reports through GitHub. [Security policy](../SECURITY.md)
- **Security → Dependabot:** enable alerts and security updates. [.github/dependabot.yml](../.github/dependabot.yml) schedules weekly version updates for Cargo and GitHub Actions.

Push the reviewed source to the default branch. To sign the native Swift app, configure Developer ID and notarization credentials in the `release-signing` environment. Use the certificate's identity name from `security find-identity -v -p codesigning`.

| Environment secret | Value |
| --- | --- |
| `MACOS_CERTIFICATE_BASE64` | Base64-encoded Developer ID Application certificate and private key export in PKCS#12 format. |
| `MACOS_CERTIFICATE_PASSWORD` | Password used to export that PKCS#12 file. |
| `MACOS_SIGN_IDENTITY` | Full Developer ID Application identity. |
| `APPLE_API_KEY_BASE64` | Base64-encoded App Store Connect team API key file used by `notarytool`. |
| `APPLE_API_KEY_ID` | Key ID. |
| `APPLE_API_ISSUER` | Issuer UUID. |

Tag the reviewed commit with the version prefixed by `v`. Run **Release artifacts** from that tag. The workflow runs the macOS and Linux checks and uploads the eight tested CLI archives, Basic and Pro for each target, with their checksums and the installer script. Enable **Also sign and notarize the native Swift Mac app** to get a Developer ID-signed ZIP with a stapled notarization ticket.

Review the Apple notary log and verify the checksums. Test the archived app on a fresh Mac and the CLI archives on their matching architectures. Publish the approved artifacts and release notes to the versioned GitHub Release. Upload each archive and matching `.sha256` file as a separate asset, keeping the exact generated filenames. The installer locates them through the release's stable asset names.

The workflow produces review artifacts. The owner publishes them after review. The [contribution issue drafts](contribution-issues.md) are ready for the repository issue tracker.

## Sign and notarize on a Mac

Install Xcode and the Developer ID Application signing identity. Store credentials with `xcrun notarytool store-credentials` in a local Keychain profile. Keep credentials out of the repository.

```sh
scripts/package-macos.sh --universal
MACOS_SIGN_IDENTITY='Developer ID Application: Example (TEAMID)' \
NOTARY_PROFILE='diskuse16x-notary' \
  scripts/notarize-macos.sh "dist/DiskUse16x.app"
```

The notarization script signs a copy of the app and places the distributable ZIP, checksum, and notary log under `dist`. The input bundle remains available for local QA.

- [Apple notarization guide](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution/)
- [Apple packaging guide](https://developer.apple.com/documentation/xcode/packaging-mac-software-for-distribution)
- [GitHub guide to importing signing identities on macOS runners](https://docs.github.com/en/actions/how-tos/deploy/deploy-to-third-party-platforms/sign-xcode-applications)

## Package the CLI locally

Build on a host matching one of the [CLI release targets](install.md#candidate-cli-targets). Install the Rustup toolchain and the target's Rust standard library. On Linux, install the platform's musl development tools and binutils. Set the target string for your host and run the packaging script from the checkout.

```sh
target=x86_64-unknown-linux-musl
(cd cli && rustup target add "$target")
scripts/package-cli.sh "$target"
scripts/package-cli.sh --pro "$target"
```

The script checks the executable's version, confirms its macOS architecture or Linux static linkage, and smoke-tests a scan before creating `dist/diskuse16x-<target>.tar.gz` and its SHA-256 file. `--pro` builds the Pro edition with `--features pro` into `dist/diskuse16x-pro-<target>.tar.gz`, and the version must end with `(pro)`. Mac CLI archives use an ad hoc signature. The universal native app follows the Developer ID and notarization procedure above.

Run `scripts/check-cli.py` and `scripts/check-tui.py` against each edition's executable extracted from its archive. Both scripts read the edition from `--version`. Use a standard user account to exercise permission errors.
