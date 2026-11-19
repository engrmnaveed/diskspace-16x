#!/bin/sh
# Install a checksummed Space 16X CLI release from a public GitHub repository.
set -eu

usage() {
    cat <<'USAGE'
Usage: install.sh [--repo OWNER/REPO] [--version VERSION] [--bin-dir DIR] [--shell-path]
       install.sh --uninstall [--shell-path]

Download the CLI for this OS and architecture, verify its SHA-256 checksum,
and install it without sudo. The defaults are the latest release of
engrmnaveed/diskspace-16x and $HOME/.local/share/space16x/bin.
--shell-path prints the updated PATH for the calling shell.
USAGE
}

fail() {
    printf 'space16x install: %s\n' "$*" >&2
    exit 1
}

repo=engrmnaveed/diskspace-16x
version=
bin_dir=
action=install
print_path=false
while [ "$#" -gt 0 ]; do
    case "$1" in
        -h|--help)
            usage
            exit 0
            ;;
        --uninstall) action=uninstall; shift ;;
        --shell-path) print_path=true; shift ;;
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

[ -n "${HOME:-}" ] || fail "Set HOME before running the installer."
state=$HOME/.local/share/space16x
receipt=$state/receipt
profiles=$state/profiles
created_dirs=$state/dirs
tab=$(printf '\t')
newline='
'
stage=
staged_binary=
locked=false
installed_dir=
installed_digest=
path_added=false

cleanup() {
    if [ -n "$staged_binary" ]; then rm -f "$staged_binary"; fi
    if [ -n "$stage" ]; then rm -rf "$stage"; fi
    if "$locked"; then
        rmdir "$state/.lock" 2>/dev/null || :
        rmdir "$state/bin" "$state" 2>/dev/null || :
    fi
}
trap cleanup 0
trap 'exit 1' HUP INT TERM

safe_path() {
    case "$1" in
        *:*|*"$tab"*|*"$newline"*) fail "Install paths cannot contain a colon, tab or newline." ;;
    esac
}

lock_installation() {
    [ ! -L "$state" ] || fail "The installer directory is a symbolic link: $state"
    mkdir -p "$state"
    mkdir "$state/.lock" 2>/dev/null || fail "Another Space 16X installer is active."
    locked=true
    for managed in "$receipt" "$profiles" "$created_dirs" "$state/env.sh" "$state/env.fish"; do
        [ ! -L "$managed" ] || fail "An installer state file is a symbolic link: $managed"
    done
    if [ -f "$receipt" ]; then
        { IFS= read -r installed_dir && IFS= read -r installed_digest && IFS= read -r path_added; } < "$receipt" ||
            fail "The install receipt is incomplete: $receipt"
        safe_path "$installed_dir"
        case "$installed_dir:$path_added" in /*:true|/*:false) ;; *) fail "Invalid install receipt." ;; esac
    fi
}

checksum_command() {
    if command -v sha256sum >/dev/null 2>&1; then
        checksum_tool=sha256sum
    elif command -v shasum >/dev/null 2>&1; then
        checksum_tool=shasum
    else
        fail "Install sha256sum or shasum to verify the executable."
    fi
}

digest() {
    case "$checksum_tool" in
        sha256sum) value=$(sha256sum < "$1") || return 1 ;;
        shasum) value=$(shasum -a 256 < "$1") || return 1 ;;
    esac
    printf '%s\n' "${value%% *}"
}

profile_line() {
    case "$1" in
        fish) printf '%s\n' 'test -r "$HOME/.local/share/space16x/env.fish"; and source "$HOME/.local/share/space16x/env.fish" # Space 16X installer' ;;
        *) printf '%s\n' '[ ! -r "$HOME/.local/share/space16x/env.sh" ] || . "$HOME/.local/share/space16x/env.sh" # Space 16X installer' ;;
    esac
}

profile_target() {
    current_profile=$1
    links=0
    while [ -L "$current_profile" ]; do
        [ "$links" -lt 32 ] || return 1
        link=$(readlink "$current_profile") || return 1
        case "$link" in
            /*) current_profile=$link ;;
            *) current_profile=${current_profile%/*}/$link ;;
        esac
        links=$((links + 1))
    done
    parent=$(cd "${current_profile%/*}" && pwd -P) || return 1
    printf '%s/%s\n' "$parent" "${current_profile##*/}"
}

last_byte() {
    tail -c 1 "$1" | od -An -tu1 | tr -d ' \n'
}

ensure_directory() {
    [ -d "$1" ] && return 0
    safe_path "$1"
    directory_parent=${1%/*}
    ensure_directory "${directory_parent:-/}"
    mkdir "$1"
    printf '%s\n' "$1" >> "$created_dirs"
}

configure_profile() {
    profile_type=$1
    ensure_directory "${2%/*}"
    profile=$(profile_target "$2") || fail "Cannot resolve shell profile: $2"
    safe_path "$profile"
    line=$(profile_line "$profile_type")
    prior=existing
    if [ -e "$profile" ]; then
        [ -f "$profile" ] && [ -w "$profile" ] || fail "Cannot update shell profile: $profile"
        if grep -Fqx "$line" "$profile"; then return; fi
        if [ -s "$profile" ] && [ "$(last_byte "$profile")" != 10 ]; then prior=no-newline; fi
    else
        prior=new
    fi
    printf '%s\t%s\t%s\n' "$profile_type" "$prior" "$profile" >> "$profiles"
    if [ "$prior" = no-newline ]; then printf '\n' >> "$profile"; fi
    printf '%s\n' "$line" >> "$profile"
}

remove_profiles() {
    [ -f "$profiles" ] || return 0
    while IFS="$tab" read -r profile_type prior profile; do
        [ -f "$profile" ] && [ ! -L "$profile" ] || continue
        line=$(profile_line "$profile_type")
        grep -Fqx "$line" "$profile" || continue
        trim=0
        if [ "$(last_byte "$profile")" != 10 ] ||
            { [ "$prior" = no-newline ] && [ "$(tail -n 1 "$profile")" = "$line" ]; }; then
            trim=1
        fi
        edited=$(mktemp "${profile%/*}/.space16x-profile.XXXXXX")
        if ! cp -p "$profile" "$edited" ||
            ! awk -v line="$line" -v trim="$trim" '
                $0 == line { next }
                have_line { print previous }
                { previous = $0; have_line = 1 }
                END { if (have_line) printf "%s%s", previous, (trim ? "" : "\n") }
            ' "$profile" > "$edited"; then
            rm -f "$edited"
            fail "Could not clean shell profile: $profile"
        fi
        if [ "$prior" = new ] && [ ! -s "$edited" ]; then
            rm -f "$edited" "$profile"
        elif ! mv -f "$edited" "$profile"; then
            rm -f "$edited"
            fail "Could not clean shell profile: $profile"
        fi
    done < "$profiles"
}

remove_from_path() {
    remaining=$PATH:
    separator=
    while [ -n "$remaining" ]; do
        item=${remaining%%:*}
        remaining=${remaining#*:}
        if [ "$item" != "$1" ]; then
            printf '%s%s' "$separator" "$item"
            separator=:
        fi
    done
    printf '\n'
}

safe_path "$state"
if [ "$action" = uninstall ]; then
    if [ -f "$receipt" ]; then
        lock_installation
        destination=$installed_dir/space16x
        if [ -f "$destination" ] && [ ! -L "$destination" ]; then
            checksum_command
            if [ "$(digest "$destination")" = "$installed_digest" ]; then
                rm -f "$destination"
            else
                printf 'Kept the replacement executable at %s\n' "$destination" >&2
            fi
        fi
        remove_profiles
        if [ -f "$created_dirs" ]; then
            awk '{ dirs[NR] = $0 } END { for (i = NR; i > 0; i--) print dirs[i] }' "$created_dirs" |
                while IFS= read -r directory; do rmdir "$directory" 2>/dev/null || :; done
        fi
        rm -f "$receipt" "$profiles" "$created_dirs" "$state/env.sh" "$state/env.fish"
        printf 'Removed the Space 16X installation and its shell setup.\n' >&2
    else
        printf 'No managed Space 16X installation was found.\n' >&2
    fi
    if "$print_path"; then
        if "$path_added"; then remove_from_path "$installed_dir"; else printf '%s\n' "$PATH"; fi
    fi
    exit 0
fi

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

checksum_command
stage=$(mktemp -d "${TMPDIR:-/tmp}/space16x-install.XXXXXX")

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
[ "$(digest "$stage/$asset")" = "$expected" ] || fail "SHA-256 checksum mismatch for $asset."

# Stream only the executable into our staging directory, without unpacking paths.
tar -xOzf "$stage/$asset" space16x > "$stage/space16x" ||
    fail "The release archive does not contain a readable space16x executable."
[ -s "$stage/space16x" ] || fail "The release executable is empty."

lock_installation
if [ -z "$bin_dir" ]; then bin_dir=${installed_dir:-$state/bin}; fi
case "$bin_dir" in /*) ;; *) bin_dir=$PWD/$bin_dir ;; esac
safe_path "$bin_dir"
ensure_directory "$bin_dir"
bin_dir=$(cd "$bin_dir" && pwd -P)
safe_path "$bin_dir"
if [ -n "$installed_dir" ] && [ "$bin_dir" != "$installed_dir" ]; then
    fail "Space 16X is installed in $installed_dir. Uninstall it before choosing a new directory."
fi
destination=$bin_dir/space16x
if [ -e "$destination" ] || [ -L "$destination" ]; then
    [ -f "$destination" ] && [ ! -L "$destination" ] &&
        [ "$bin_dir" = "$installed_dir" ] && [ "$(digest "$destination")" = "$installed_digest" ] ||
        fail "An unmanaged file already exists at $destination."
fi

new_path=$PATH
case ":$PATH:" in
    *":$bin_dir:"*) ;;
    *) new_path=$bin_dir${PATH:+:$PATH}; path_added=true ;;
esac
staged_binary=$(mktemp "$bin_dir/.space16x-install.XXXXXX")
cp "$stage/space16x" "$staged_binary"
chmod 0755 "$staged_binary"
new_digest=$(digest "$staged_binary")
mv -f "$staged_binary" "$destination"
staged_binary=
printf '%s\n%s\n%s\n' "$bin_dir" "$new_digest" "$path_added" > "$receipt"

if "$path_added"; then
    cat > "$state/env.sh" <<'ENV'
if IFS= read -r space16x_bin < "$HOME/.local/share/space16x/receipt"; then
    case ":$PATH:" in
        *":$space16x_bin:"*) ;;
        *) export PATH="$space16x_bin${PATH:+:$PATH}" ;;
    esac
    unset space16x_bin
fi
ENV
    login_shell=${SHELL:-sh}
    case "${login_shell##*/}" in
        bash)
            configure_profile sh "$HOME/.bashrc"
            login=$HOME/.profile
            for candidate in "$HOME/.bash_profile" "$HOME/.bash_login"; do
                if [ -f "$candidate" ]; then login=$candidate; break; fi
            done
            configure_profile sh "$login"
            ;;
        zsh) configure_profile sh "${ZDOTDIR:-$HOME}/.zshrc" ;;
        fish)
            cat > "$state/env.fish" <<'ENV'
if read -l space16x_bin < "$HOME/.local/share/space16x/receipt"
    if not contains -- "$space16x_bin" $PATH
        set -gx PATH "$space16x_bin" $PATH
    end
end
ENV
            configure_profile fish "${XDG_CONFIG_HOME:-$HOME/.config}/fish/config.fish"
            ;;
        *) configure_profile sh "$HOME/.profile" ;;
    esac
fi
printf 'Installed %s\n' "$destination" >&2
if "$print_path"; then printf '%s\n' "$new_path"; fi
