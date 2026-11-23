#!/bin/sh
# Install the Homebrew formulas from local archives: render du16x and
# du16x-pro into a temporary tap with file:// URLs, check their style,
# install, test and uninstall each, and check that they refuse to install together.
# Usage: scripts/check-formula.sh TARGET, after scripts/package-cli.sh built
# dist/diskuse16x-TARGET.tar.gz and dist/diskuse16x-pro-TARGET.tar.gz.
set -eu

[ "$#" -eq 1 ] || { echo "Usage: $0 TARGET" >&2; exit 1; }
target=$1
cd "$(dirname "$0")/.."
project="$PWD"
version="$(cat scripts/release-version)"
export HOMEBREW_NO_AUTO_UPDATE=1 HOMEBREW_NO_ANALYTICS=1 HOMEBREW_NO_INSTALL_CLEANUP=1 HOMEBREW_NO_ENV_HINTS=1

tap_name=local/diskuse16x-formula-check
digests="$(mktemp -d)"
cleanup() {
    brew uninstall --formula "$tap_name/du16x" "$tap_name/du16x-pro" >/dev/null 2>&1 || true
    brew untrust "$tap_name" >/dev/null 2>&1 || true
    brew untap "$tap_name" >/dev/null 2>&1 || true
    rm -rf "$digests"
}
trap cleanup 0
# The formula lists all four targets. Only this host's archive exists here, so
# its digest stands in for the others, which this host never downloads.
for archive in diskuse16x diskuse16x-pro; do
    for other in aarch64-apple-darwin x86_64-apple-darwin aarch64-unknown-linux-musl x86_64-unknown-linux-musl; do
        cp "dist/$archive-$target.tar.gz.sha256" "$digests/$archive-$other.tar.gz.sha256"
    done
done

brew tap-new --no-git "$tap_name" >/dev/null
tap="$(brew --repository "$tap_name")"
mkdir -p "$tap/Formula"
scripts/render-formula --url-base "file://$project/dist" "$version" "$digests" > "$tap/Formula/du16x.rb"
scripts/render-formula --pro --url-base "file://$project/dist" "$version" "$digests" > "$tap/Formula/du16x-pro.rb"
brew style "$tap_name/du16x" "$tap_name/du16x-pro"
# Homebrew 6 and later load formulas only from trusted taps.
if brew commands | grep -qx trust; then brew trust "$tap_name"; fi

brew install --formula "$tap_name/du16x"
brew test "$tap_name/du16x"
if brew install --formula "$tap_name/du16x-pro" >/dev/null 2>&1; then
    echo "Homebrew installed du16x-pro beside du16x." >&2
    exit 1
fi
brew uninstall --formula "$tap_name/du16x"
brew install --formula "$tap_name/du16x-pro"
brew test "$tap_name/du16x-pro"
[ "$("$(brew --prefix)/bin/du16x" --version)" = "diskuse16x $version (pro)" ]
brew uninstall --formula "$tap_name/du16x-pro"
echo "Homebrew formulas for $target: style, install, test and conflict work."
