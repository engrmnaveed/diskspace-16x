# Release runbook

## Review before publication

Confirm the product name, runtime coverage for the release matrix, distribution channels, and company-site placement with the project owner. Record the scan benchmark on named hardware. Exercise the [macOS manual checks](manual-qa.md), including the macOS 14 deployment floor, before claiming support. Hosted CI is configured for Apple silicon and Intel on macOS 15, plus the CLI on Ubuntu 24.04 x86-64 and ARM64.

Check that [scripts/release-version](../scripts/release-version), [cli/Cargo.toml](../cli/Cargo.toml), and the CLI's `--version` output agree. Update the [changelog](../CHANGELOG.md), the [installation guide](install.md) package examples, and the release notes as needed.

## Build release artifacts in GitHub Actions

Pushing a new `vX.Y.Z` tag that matches `scripts/release-version` runs **Publish CLI**. The workflow tests the four Rust targets and publishes their checksummed archives to the tagged GitHub release. The Mac app signing workflow is separate.

Push the reviewed source to the default branch. To sign the native Swift app, configure Developer ID and notarization credentials in the `release-signing` environment. Use the certificate's identity name from `security find-identity -v -p codesigning`.

| Environment secret | Value |
| --- | --- |
| `MACOS_CERTIFICATE_BASE64` | Base64-encoded Developer ID Application certificate and private key export in PKCS#12 format. |
| `MACOS_CERTIFICATE_PASSWORD` | Password used to export that PKCS#12 file. |
| `MACOS_SIGN_IDENTITY` | Full Developer ID Application identity. |
| `APPLE_API_KEY_BASE64` | Base64-encoded App Store Connect team API key file used by `notarytool`. |
| `APPLE_API_KEY_ID` | Key ID. |
| `APPLE_API_ISSUER` | Issuer UUID. |

Tag the reviewed commit with the version prefixed by `v`. Run **Release artifacts** from that tag. The workflow runs the macOS and Linux checks and uploads the four tested CLI archives with their checksums and the installer script. Enable **Also sign and notarize the native Swift Mac app** to get a Developer ID-signed ZIP with a stapled notarization ticket.

Review the Apple notary log and verify the checksums. Test the archived app on a fresh Mac and the CLI archives on their matching architectures. Publish the approved artifacts and release notes to the versioned GitHub Release. Upload each archive and matching `.sha256` file as a separate asset, keeping the exact generated filenames. The installer locates them through the release's stable asset names.

The workflow produces review artifacts. The owner publishes them after review. The [contribution issue drafts](contribution-issues.md) are ready for the repository issue tracker.

## Sign and notarize on a Mac

Install Xcode and the Developer ID Application signing identity. Store credentials with `xcrun notarytool store-credentials` in a local Keychain profile. Keep credentials out of the repository.

```sh
scripts/package-macos.sh --universal
MACOS_SIGN_IDENTITY='Developer ID Application: Example (TEAMID)' \
NOTARY_PROFILE='space16x-notary' \
  scripts/notarize-macos.sh "dist/Space 16X.app"
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
```

The script checks the executable's version, confirms its macOS architecture or Linux static linkage, and smoke-tests a scan before creating `dist/space16x-<target>.tar.gz` and its SHA-256 file. Mac CLI archives use an ad hoc signature. The universal native app follows the Developer ID and notarization procedure above.

Run `scripts/check-cli.py` and `scripts/check-tui.py` against the executable extracted from the archive. Use a standard user account to exercise permission errors.
