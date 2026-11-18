#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
if [[ "$(uname -s)" != Linux || "$(uname -m)" != x86_64 ]]; then
    echo "Build the release CLI on Ubuntu 24.04 x86-64." >&2
    exit 1
fi
# shellcheck source=/dev/null
source /etc/os-release
if [[ "$ID" != ubuntu || "$VERSION_ID" != 24.04 ]]; then
    echo "Build the release CLI on Ubuntu 24.04 x86-64." >&2
    exit 1
fi

version="$(cat scripts/release-version)"
name="space16x-$version-linux-x86_64"
swift build -c release --product space16x --static-swift-stdlib >&2
bin_dir="$(swift build -c release --show-bin-path)"
mkdir -p dist
stage="$(mktemp -d "$PWD/dist/.space16x-linux.XXXXXX")"
trap 'rm -rf "$stage"' EXIT

mkdir "$stage/$name"
install -m 0755 "$bin_dir/space16x" "$stage/$name/space16x"
cli="$stage/$name/space16x"
if [[ "$("$cli" --version)" != "space16x $version" ]]; then
    echo "The CLI version does not match scripts/release-version." >&2
    exit 1
fi
"$cli" scan "$stage/$name" --top 0 --json >&2
cp docs/cli.md docs/example-report.json "$stage/$name/"
if [[ -f LICENSE ]]; then
    cp LICENSE "$stage/$name/"
fi

tar -C "$stage" -czf "$stage/$name.tar.gz" "$name"
(cd "$stage" && sha256sum "$name.tar.gz" > "$name.tar.gz.sha256")
mv "$stage/$name.tar.gz" "$stage/$name.tar.gz.sha256" dist/
printf '%s\n' "$PWD/dist/$name.tar.gz"
