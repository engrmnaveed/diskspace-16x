#!/bin/sh
# Install the Homebrew formulas from local archives: render space16x-cli and
# space16x-cli-pro into a temporary tap with file:// URLs, check their style,
# install, test and uninstall each, and check that they refuse to install together.
# Usage: scripts/check-formula.sh TARGET, after scripts/package-cli.sh built
# dist/space16x-TARGET.tar.gz and dist/space16x-pro-TARGET.tar.gz.
set -eu

[ "$#" -eq 1 ] || { echo "Usage: $0 TARGET" >&2; exit 1; }
target=$1
cd "$(dirname "$0")/.."
project="$PWD"
version="$(cat scripts/release-version)"
export HOMEBREW_NO_AUTO_UPDATE=1 HOMEBREW_NO_ANALYTICS=1 HOMEBREW_NO_INSTALL_CLEANUP=1 HOMEBREW_NO_ENV_HINTS=1

tap_name=local/space16x-formula-check
digests="$(mktemp -d)"
cleanup() {
    brew uninstall --formula "$tap_name/space16x-cli" "$tap_name/space16x-cli-pro" >/dev/null 2>&1 || true
    brew untrust "$tap_name" >/dev/null 2>&1 || true
    brew untap "$tap_name" >/dev/null 2>&1 || true
    rm -rf "$digests"
}
trap cleanup 0
# The formula lists all four targets. Only this host's archive exists here, so
# its digest stands in for the others, which this host never downloads.
for archive in space16x space16x-pro; do
    for other in aarch64-apple-darwin x86_64-apple-darwin aarch64-unknown-linux-musl x86_64-unknown-linux-musl; do
        cp "dist/$archive-$target.tar.gz.sha256" "$digests/$archive-$other.tar.gz.sha256"
    done
done

brew tap-new --no-git "$tap_name" >/dev/null
tap="$(brew --repository "$tap_name")"
mkdir -p "$tap/Formula"
scripts/render-formula --url-base "file://$project/dist" "$version" "$digests" > "$tap/Formula/space16x-cli.rb"
scripts/render-formula --pro --url-base "file://$project/dist" "$version" "$digests" > "$tap/Formula/space16x-cli-pro.rb"
brew style "$tap_name/space16x-cli" "$tap_name/space16x-cli-pro"
# Homebrew 6 and later load formulas only from trusted taps.
if brew commands | grep -qx trust; then brew trust "$tap_name"; fi

brew install --formula "$tap_name/space16x-cli"
brew test "$tap_name/space16x-cli"
if brew install --formula "$tap_name/space16x-cli-pro" >/dev/null 2>&1; then
    echo "Homebrew installed space16x-cli-pro beside space16x-cli." >&2
    exit 1
fi
brew uninstall --formula "$tap_name/space16x-cli"
brew install --formula "$tap_name/space16x-cli-pro"
brew test "$tap_name/space16x-cli-pro"
[ "$("$(brew --prefix)/bin/space16x" --version)" = "space16x $version (pro)" ]
brew uninstall --formula "$tap_name/space16x-cli-pro"
echo "Homebrew formulas for $target: style, install, test and conflict work."
