#!/bin/sh
# Install a checksummed Space 16X CLI release from a public GitHub repository.
set -eu

usage() {
    cat <<'USAGE'
Usage: install.sh [--repo OWNER/REPO] [--version VERSION] [--bin-dir DIR]

Download the CLI for this OS and architecture, verify its SHA-256 checksum,
and install it without sudo. The defaults are the latest release of
engrmnaveed/diskspace-16x and $HOME/.local/bin. Versions may start with v.
USAGE
}

fail() {
    printf 'space16x install: %s\n' "$*" >&2
    exit 1
}

repo=engrmnaveed/diskspace-16x
version=
bin_dir=
while [ "$#" -gt 0 ]; do
    case "$1" in
        -h|--help)
            usage
            exit 0
            ;;
        --repo|--version|--bin-dir)
            [ "$#" -ge 2 ] && [ -n "$2" ] || fail "$1 requires a value."
            case "$1" in
                --repo) repo=$2 ;;
                --version) version=$2 ;;
                --bin-dir) bin_dir=$2 ;;
            esac
            shift 2
            ;;
        *) fail "Unknown argument '$1'. Run install.sh --help." ;;
    esac
done

case "$repo" in
    */*) owner=${repo%%/*}; project=${repo#*/} ;;
    *) fail "Use --repo OWNER/REPO for a public GitHub repository." ;;
esac
case "$owner" in
    ''|*[!A-Za-z0-9-]*) fail "Invalid GitHub owner in --repo." ;;
esac
case "$project" in
    ''|.|..|*[!A-Za-z0-9_.-]*) fail "Invalid GitHub repository in --repo." ;;
esac

release=latest/download
if [ -n "$version" ]; then
    version=${version#v}
    printf '%s\n' "$version" | LC_ALL=C grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9]+([.-][A-Za-z0-9]+)*)?$' ||
        fail "Use a release version such as 0.2.0 or v0.2.0."
    # grep works line by line. Check the entire parameter before constructing a URL.
    case "$version" in *[!A-Za-z0-9.-]*) fail "Invalid characters in --version." ;; esac
    release=download/v$version
fi

host_os=$(uname -s)
host_arch=$(uname -m)
case "$host_os:$host_arch" in
    Darwin:arm64|Darwin:aarch64) target=aarch64-apple-darwin ;;
    Darwin:x86_64|Darwin:amd64) target=x86_64-apple-darwin ;;
    Linux:arm64|Linux:aarch64) target=aarch64-unknown-linux-musl ;;
    Linux:x86_64|Linux:amd64) target=x86_64-unknown-linux-musl ;;
    *) fail "No prebuilt CLI for $host_os $host_arch. See docs/install.md for a source build." ;;
esac
if [ "$host_os" = Darwin ]; then
    macos_version=$(sw_vers -productVersion) || fail "Could not determine the macOS version."
    major=${macos_version%%.*}
    case "$major" in ''|*[!0-9]*) fail "Could not determine the macOS version." ;; esac
    [ "$major" -ge 14 ] || fail "The prebuilt CLI requires macOS 14 or later. Found $macos_version."
fi

if command -v curl >/dev/null 2>&1; then
    downloader=curl
elif command -v wget >/dev/null 2>&1 && wget --help 2>&1 | grep -q -- '--max-redirect'; then
    downloader=wget
else
    fail "Install curl or GNU Wget to download the release over HTTPS."
fi

if command -v sha256sum >/dev/null 2>&1; then
    checksum_tool=sha256sum
elif command -v shasum >/dev/null 2>&1; then
    checksum_tool=shasum
else
    fail "Install sha256sum or shasum to verify the release."
fi

if [ -z "$bin_dir" ]; then
    [ -n "${HOME:-}" ] || fail "Set HOME or provide --bin-dir."
    bin_dir=$HOME/.local/bin
fi
case "$bin_dir" in /*) ;; *) bin_dir=$PWD/$bin_dir ;; esac
destination=$bin_dir/space16x
[ ! -d "$destination" ] || fail "The install path is a directory: $destination"
mkdir -p "$bin_dir" || fail "Cannot create install directory: $bin_dir"
stage=$(mktemp -d "$bin_dir/.space16x-install.XXXXXX") || fail "Cannot stage a file in $bin_dir."
trap 'rm -rf "$stage"' 0
trap 'exit 1' HUP INT TERM

download_wget() {
    request=$1
    redirects=0
    while [ "$redirects" -le 5 ]; do
        if LC_ALL=C wget --no-config --no-hsts --max-redirect=0 --server-response \
            --timeout=20 --tries=3 --output-document="$2" "$request" 2> "$stage/wget-response"; then
            return 0
        fi
        location=$(awk 'tolower($1) == "location:" { sub(/\r$/, "", $2); print $2; exit }' "$stage/wget-response")
        case "$location" in
            https://*) request=$location ;;
            //*) request=https:$location ;;
            /*)
                host=${request#https://}
                request=https://${host%%/*}$location
                ;;
            *) return 1 ;;
        esac
        redirects=$((redirects + 1))
    done
    return 1
}

download() {
    case "$downloader" in
        curl)
            curl --disable --fail --silent --show-error --location --proto '=https' --proto-redir '=https' \
                --retry 2 --connect-timeout 15 --max-time 180 --output "$2" "$1"
            ;;
        wget)
            download_wget "$1" "$2"
            ;;
    esac
}

asset=space16x-$target.tar.gz
base=https://github.com/$repo/releases/$release
download "$base/$asset.sha256" "$stage/$asset.sha256" ||
    fail "Could not download $asset.sha256 from $base. Check that this release publishes the requested target."
download "$base/$asset" "$stage/$asset" ||
    fail "Could not download $asset from $base."

IFS=' ' read -r expected filename extra < "$stage/$asset.sha256" ||
    fail "The release checksum file is empty or incomplete."
case "$filename" in
    "$asset"|"*$asset") ;;
    *) fail "The release checksum file names a different asset." ;;
esac
[ -z "$extra" ] && [ "${#expected}" -eq 64 ] || fail "Invalid SHA-256 checksum file."
case "$expected" in *[!0-9a-fA-F]*) fail "Invalid SHA-256 checksum file." ;; esac
expected=$(printf '%s' "$expected" | LC_ALL=C tr '[:upper:]' '[:lower:]')
case "$checksum_tool" in
    sha256sum) actual=$(sha256sum < "$stage/$asset") || fail "Could not hash $asset." ;;
    shasum) actual=$(shasum -a 256 < "$stage/$asset") || fail "Could not hash $asset." ;;
esac
[ "${actual%% *}" = "$expected" ] || fail "SHA-256 checksum mismatch for $asset."

# Stream only the executable into our staging directory, without unpacking paths.
tar -xOzf "$stage/$asset" space16x > "$stage/space16x" ||
    fail "The release archive does not contain a readable space16x executable."
[ -s "$stage/space16x" ] || fail "The release executable is empty."
chmod 0755 "$stage/space16x"
mv -f "$stage/space16x" "$destination" || fail "Could not install $destination."
printf 'Installed %s\n' "$destination"
case ":${PATH:-}:" in
    *:"$bin_dir":*) ;;
    *) printf 'Add %s to PATH to run space16x by name.\n' "$bin_dir" ;;
esac
