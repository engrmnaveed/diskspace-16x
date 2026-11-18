# Release runbook

## Review before publication

Confirm the product name, MIT license, macOS floor and architectures, first Linux target, distribution channels, and company-site placement with the project owner. Record the scan benchmark on named hardware. Exercise the [macOS manual checks](manual-qa.md), including the macOS 14 deployment floor, before claiming support. Hosted CI covers Apple silicon and Intel on macOS 15 and the CLI on Ubuntu 24.04 x86-64.

Check the version in [scripts/release-version](../scripts/release-version) and the CLI's `--version` output. Update the [changelog](../CHANGELOG.md), the [installation guide](install.md) package examples, and the release notes as needed.

## Build release artifacts in GitHub Actions

After connecting an approved remote, put the reviewed source on the default branch. Create a `release-signing` environment and configure its Developer ID and notarization credentials. Use the certificate's identity name from `security find-identity -v -p codesigning`.

| Environment secret | Value |
| --- | --- |
| `MACOS_CERTIFICATE_BASE64` | Base64-encoded Developer ID Application certificate and private key export in PKCS#12 format. |
| `MACOS_CERTIFICATE_PASSWORD` | Password used to export that PKCS#12 file. |
| `MACOS_SIGN_IDENTITY` | Full Developer ID Application identity. |
| `APPLE_API_KEY_BASE64` | Base64-encoded App Store Connect team API key file used by `notarytool`. |
| `APPLE_API_KEY_ID` | Key ID. |
| `APPLE_API_ISSUER` | Issuer UUID. |

Tag the reviewed commit with the version prefixed by `v`. Run **Release artifacts** from that tag. The workflow runs the macOS and Linux checks, builds the universal app and x86-64 CLI, signs the app with hardened runtime, requests notarization, staples the ticket, and uploads checksummed archives for review.

Review the Apple notary log and verify both checksums. Test the archived Mac app on a fresh Mac and the CLI in a clean Ubuntu 24.04 x86-64 environment. Publish the approved archives and corresponding checksum files with the release notes. The workflow leaves publication to the owner. The [contribution issue drafts](contribution-issues.md) are ready for the repository issue tracker.

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

## Package Ubuntu locally

On Ubuntu 24.04 x86-64 with Swift 6.2.4, run `scripts/package-linux.sh`. This builds the CLI with the Swift runtime linked into the executable and saves a tarball and SHA-256 file in `dist`.

Run `python3 scripts/check-cli.py PATH-TO-EXTRACTED-CLI` as a standard user. Also run the extracted executable on a fresh Ubuntu 24.04 x86-64 system with no Swift toolchain installed.
