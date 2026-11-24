#!/bin/sh
set -eu

cd "$(dirname "$0")/.."
project="$PWD"
case "$(uname -s):$(uname -m)" in
    Darwin:arm64|Darwin:aarch64) native=aarch64-apple-darwin ;;
    Darwin:x86_64|Darwin:amd64) native=x86_64-apple-darwin ;;
    Linux:arm64|Linux:aarch64) native=aarch64-unknown-linux-musl ;;
    Linux:x86_64|Linux:amd64) native=x86_64-unknown-linux-musl ;;
    *) echo "No prebuilt CLI target for this host. Use Cargo for a source build." >&2; exit 1 ;;
esac
usage() {
    echo "Usage: $0 [--pro] [TARGET]. Use the native target, or build x86_64-apple-darwin on an Apple silicon Mac with Rosetta." >&2
    exit 1
}
# --pro builds the Pro edition into diskuse16x-pro-TARGET.tar.gz.
edition=
if [ "${1:-}" = --pro ]; then
    edition=pro
    shift
fi
target="${1:-$native}"
[ "$#" -le 1 ] || usage
case "$native:$target" in
    "$native:$native"|aarch64-apple-darwin:x86_64-apple-darwin) ;;
    *) usage ;;
esac

version="$(cat scripts/release-version)"
target_dir="${CARGO_TARGET_DIR:-$project/.build/cli-release}"
case "$target_dir" in /*) ;; *) target_dir="$project/$target_dir" ;; esac
if [ "$edition" = pro ]; then
    asset="diskuse16x-pro-$target.tar.gz"
    expected_version="diskuse16x $version (pro)"
    target_dir="$target_dir-pro"
    set -- --features pro
else
    asset="diskuse16x-$target.tar.gz"
    expected_version="diskuse16x $version"
    set --
fi
case "$native" in *-apple-darwin) export MACOSX_DEPLOYMENT_TARGET=14.0 ;; esac
(cd cli && cargo build --locked --release --target "$target" --target-dir "$target_dir" "$@" >&2)

mkdir -p dist
stage="$(mktemp -d "$project/dist/.diskuse16x-cli.XXXXXX")"
trap 'rm -rf "$stage"' 0
payload="$stage/payload"
mkdir "$payload"
binary="$payload/diskuse16x"
install -m 0755 "$target_dir/$target/release/diskuse16x" "$binary"
case "$native" in
    *-apple-darwin)
        architecture="${target%%-*}"
        if [ "$architecture" = aarch64 ]; then architecture=arm64; fi
        xcrun lipo "$binary" -verify_arch "$architecture"
        codesign --force --sign - "$binary" >&2
        codesign --verify --strict "$binary"
        ;;
    *)
        headers="$(LC_ALL=C readelf -l "$binary")"
        dynamic="$(LC_ALL=C readelf -d "$binary")"
        case "$headers:$dynamic" in
            *INTERP*|*NEEDED*) echo "Expected a statically linked Linux executable." >&2; exit 1 ;;
        esac
        ;;
esac

if [ "$("$binary" --version)" != "$expected_version" ]; then
    echo "The CLI version does not match scripts/release-version." >&2
    exit 1
fi
mkdir "$stage/fixture"
"$binary" scan "$stage/fixture" --json >/dev/null
# du16x is the short name, a link beside the executable.
ln -s diskuse16x "$payload/du16x"
[ "$("$payload/du16x" --version)" = "$expected_version" ] || { echo "du16x doesn't run diskuse16x." >&2; exit 1; }

cp docs/cli.md docs/example-report.json "$payload/"
set -- diskuse16x du16x cli.md example-report.json
# Only Pro ships the Pro reference, which keeps the Basic archive small.
if [ "$edition" = pro ]; then
    cp docs/cli-pro.md "$payload/"
    set -- "$@" cli-pro.md
fi
if [ -f LICENSE ]; then
    cp LICENSE "$payload/"
    set -- "$@" LICENSE
fi
tar -C "$payload" -czf "$stage/$asset" "$@"
(
    cd "$stage"
    case "$native" in
        *-apple-darwin) shasum -a 256 "$asset" ;;
        *) sha256sum "$asset" ;;
    esac
) > "$stage/$asset.sha256"
mv -f "$stage/$asset" "$stage/$asset.sha256" dist/
printf '%s\n' "$project/dist/$asset"
