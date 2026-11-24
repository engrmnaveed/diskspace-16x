#!/usr/bin/env bash
# Archive a packaged universal DiskUse16x app as a versioned ZIP with a SHA-256 file.
set -euo pipefail

if [[ "$(uname -s)" != Darwin || "$#" != 1 ]]; then
    echo "Usage on macOS: $0 APP" >&2
    exit 1
fi

app="$1"
if [[ -L "$app" || "$(plutil -extract CFBundleIdentifier raw "$app/Contents/Info.plist" 2>/dev/null)" != com.16xbrains.diskuse16x ]]; then
    echo "Expected a packaged DiskUse16x app." >&2
    exit 1
fi
app="$(cd "$(dirname "$app")" && pwd)/$(basename "$app")"
for architecture in arm64 x86_64; do
    xcrun lipo "$app/Contents/MacOS/DiskUse16x" -verify_arch "$architecture" ||
        { echo "Build the app with scripts/package-macos.sh --universal." >&2; exit 1; }
done
codesign --verify --strict "$app"

cd "$(dirname "$0")/.."
version="$(cat scripts/release-version)"
if [[ "$(plutil -extract CFBundleShortVersionString raw "$app/Contents/Info.plist")" != "$version" ]]; then
    echo "Expected a DiskUse16x app built from packaging version $version." >&2
    exit 1
fi

name="diskuse16x-$version-macos-universal.zip"
mkdir -p dist
stage="$(mktemp -d "$PWD/dist/.diskuse16x-archive.XXXXXX")"
trap 'rm -rf "$stage"' EXIT
ditto -c -k --keepParent "$app" "$stage/$name"
(cd "$stage" && shasum -a 256 "$name" > "$name.sha256")
mv -f "$stage/$name" "$stage/$name.sha256" dist/
printf '%s\n' "$PWD/dist/$name"
