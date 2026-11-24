#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

if [[ "$(uname -s)" != Darwin ]]; then
    echo "Build the DiskUse16x app on macOS." >&2
    exit 1
fi

case "${1:-}" in
    "") ;;
    --universal) ;;
    *) echo "Usage: $0 [--universal]" >&2; exit 1 ;;
esac
if [[ "$#" -gt 1 ]]; then
    echo "Usage: $0 [--universal]" >&2
    exit 1
fi

version="$(cat scripts/release-version)"
icon=Branding/AppIcon.icns
[[ -s "$icon" ]] || { echo "Missing app icon at $icon." >&2; exit 1; }

mkdir -p dist
stage="$(mktemp -d "$PWD/dist/.diskuse16x-macos.XXXXXX")"
trap 'rm -rf "$stage"' EXIT
app="$stage/DiskUse16x.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"

build_for_arch() {
    local architecture="$1"
    local scratch=".build/package-$architecture"
    swift build -c release --product DiskUse16xApp --arch "$architecture" --scratch-path "$scratch" >&2
    swift build -c release --show-bin-path --arch "$architecture" --scratch-path "$scratch"
}

executable="$app/Contents/MacOS/DiskUse16x"
if [[ "${1:-}" == --universal ]]; then
    arm_dir="$(build_for_arch arm64)"
    intel_dir="$(build_for_arch x86_64)"
    xcrun lipo -create "$arm_dir/DiskUse16xApp" "$intel_dir/DiskUse16xApp" -output "$executable"
    xcrun lipo "$executable" -verify_arch arm64
    xcrun lipo "$executable" -verify_arch x86_64
else
    bin_dir="$(build_for_arch "$(uname -m)")"
    install -m 0755 "$bin_dir/DiskUse16xApp" "$executable"
fi
chmod 0755 "$executable"

plist="$app/Contents/Info.plist"
cp scripts/macos/Info.plist "$plist"
plutil -insert CFBundleShortVersionString -string "$version" "$plist"
plutil -insert CFBundleVersion -string "$version" "$plist"
plutil -lint "$plist" >/dev/null
cp "$icon" "$app/Contents/Resources/AppIcon.icns"
if [[ -f LICENSE ]]; then
    cp LICENSE "$app/Contents/Resources/"
fi

codesign --force --sign - "$app" >&2
codesign --verify --strict "$app"

output="$PWD/dist/DiskUse16x.app"
if [[ -e "$output" || -L "$output" ]]; then
    if [[ -L "$output" ]] ||
        [[ "$(plutil -extract CFBundleIdentifier raw "$output/Contents/Info.plist" 2>/dev/null)" != com.16xbrains.diskuse16x ]]; then
        echo "Refusing to replace an unrecognized bundle at $output." >&2
        exit 1
    fi
    mv "$output" "$stage/previous.app"
fi
if ! mv "$app" "$output"; then
    if [[ -d "$stage/previous.app" ]]; then
        mv "$stage/previous.app" "$output"
    fi
    exit 1
fi
printf '%s\n' "$output"
