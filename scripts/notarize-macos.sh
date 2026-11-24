#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != Darwin || "$#" != 1 ]]; then
    echo "Usage on macOS: MACOS_SIGN_IDENTITY='Developer ID Application: …' NOTARY_PROFILE=… $0 APP" >&2
    exit 1
fi

: "${MACOS_SIGN_IDENTITY:?Set MACOS_SIGN_IDENTITY to the Developer ID Application identity.}"
if [[ "$MACOS_SIGN_IDENTITY" != "Developer ID Application: "* ]]; then
    echo "Use a Developer ID Application signing identity." >&2
    exit 1
fi

if [[ -n "${NOTARY_PROFILE:-}" ]]; then
    notary_args=(--keychain-profile "$NOTARY_PROFILE")
else
    : "${APPLE_API_KEY_PATH:?Set NOTARY_PROFILE or provide the three Apple API key variables.}"
    : "${APPLE_API_KEY_ID:?Set APPLE_API_KEY_ID.}"
    : "${APPLE_API_ISSUER:?Set APPLE_API_ISSUER.}"
    notary_args=(--key "$APPLE_API_KEY_PATH" --key-id "$APPLE_API_KEY_ID" --issuer "$APPLE_API_ISSUER")
fi

app="$1"
if [[ "$(plutil -extract CFBundleIdentifier raw "$app/Contents/Info.plist" 2>/dev/null)" != com.16xbrains.diskuse16x ]]; then
    echo "Expected a packaged DiskUse16x app." >&2
    exit 1
fi
app="$(cd "$(dirname "$app")" && pwd)/$(basename "$app")"
for architecture in arm64 x86_64; do
    xcrun lipo "$app/Contents/MacOS/DiskUse16x" -verify_arch "$architecture"
done
xcrun --find notarytool >/dev/null
xcrun --find stapler >/dev/null

cd "$(dirname "$0")/.."
version="$(cat scripts/release-version)"
if [[ "$(plutil -extract CFBundleShortVersionString raw "$app/Contents/Info.plist")" != "$version" ]]; then
    echo "Expected a DiskUse16x app built from packaging version $version." >&2
    exit 1
fi
name="diskuse16x-$version-macos-universal.zip"
mkdir -p dist
stage="$(mktemp -d "$PWD/dist/.diskuse16x-sign.XXXXXX")"
trap 'rm -rf "$stage"' EXIT
signed_app="$stage/DiskUse16x.app"
ditto "$app" "$signed_app"

codesign --force --options runtime --timestamp --sign "$MACOS_SIGN_IDENTITY" "$signed_app"
codesign --verify --strict "$signed_app"
ditto -c -k --keepParent "$signed_app" "$stage/submission.zip"
if ! xcrun notarytool submit "$stage/submission.zip" "${notary_args[@]}" --wait --timeout 20m --output-format json > "$stage/submission.json"; then
    cat "$stage/submission.json" >&2
    exit 1
fi
submission_id="$(plutil -extract id raw "$stage/submission.json")"
xcrun notarytool log "$submission_id" "${notary_args[@]}" "$stage/notary-log.json"
if [[ "$(plutil -extract status raw "$stage/submission.json")" != Accepted ]]; then
    cat "$stage/notary-log.json" >&2
    exit 1
fi
xcrun stapler staple "$signed_app"
xcrun stapler validate "$signed_app"
spctl --assess --type execute --verbose "$signed_app"

ditto -c -k --keepParent "$signed_app" "$stage/$name"
(cd "$stage" && shasum -a 256 "$name" > "$name.sha256")
mv "$stage/$name" "$stage/$name.sha256" dist/
cp "$stage/notary-log.json" dist/macos-notary-log.json
printf '%s\n' "$PWD/dist/$name"
