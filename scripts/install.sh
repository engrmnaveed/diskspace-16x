#!/bin/sh
# Install a checksummed DiskUse16x CLI release from a public GitHub repository.
# It installs the diskuse16x command and du16x, a link to it, and replaces an
# installation of the earlier space16x command that this installer made.
# On a Mac, --app installs the DiskUse16x app into ~/Applications instead.
set -eu

usage() {
    cat <<'USAGE'
Usage: install.sh [--pro] [--repo OWNER/REPO] [--version VERSION] [--bin-dir DIR] [--shell-path]
       install.sh --uninstall [--shell-path]
       install.sh --app [--repo OWNER/REPO] [--version VERSION]
       install.sh --uninstall --app

Download the CLI for this OS and architecture, verify its SHA-256 checksum,
and install it without sudo. The defaults are the latest release of
engrmnaveed/diskuse16x and $HOME/.local/share/diskuse16x/bin.
--pro installs DiskUse16x Pro, which replaces Basic. Run without it to switch back.
--shell-path prints the updated PATH for the calling shell.
--app installs the Mac app into ~/Applications, from the release's ZIP after
checking its SHA-256 checksum and signature. It replaces only an earlier copy
of DiskUse16x there. --uninstall --app removes that copy and keeps settings.
USAGE
}

fail() {
    printf 'diskuse16x install: %s\n' "$*" >&2
    exit 1
}

repo=engrmnaveed/diskuse16x
version=
bin_dir=
action=install
print_path=false
edition=
app=false
while [ "$#" -gt 0 ]; do
    case "$1" in
        -h|--help)
            usage
            exit 0
            ;;
        --uninstall) action=uninstall; shift ;;
        --pro) edition=pro-; shift ;;
        --app) app=true; shift ;;
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
if "$app" && { [ -n "$edition" ] || [ -n "$bin_dir" ]; }; then
    fail "--app installs the Mac app, which has Pro built in, so it takes no --pro or --bin-dir."
fi
state=$HOME/.local/share/diskuse16x
# Releases before 1.0 installed the space16x command from here.
legacy_state=$HOME/.local/share/space16x
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
    mkdir "$state/.lock" 2>/dev/null || fail "Another DiskUse16x installer is active."
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

# The line a shell profile gets: $1 is sh or fish, and $2 the state folder's name,
# diskuse16x, or space16x for an installation made before 1.0.
profile_line() {
    case "$2" in space16x) label='Space 16X' ;; *) label=DiskUse16x ;; esac
    case "$1" in
        fish) printf 'test -r "$HOME/.local/share/%s/env.fish"; and source "$HOME/.local/share/%s/env.fish" # %s installer\n' "$2" "$2" "$label" ;;
        *) printf '[ ! -r "$HOME/.local/share/%s/env.sh" ] || . "$HOME/.local/share/%s/env.sh" # %s installer\n' "$2" "$2" "$label" ;;
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
    line=$(profile_line "$profile_type" diskuse16x)
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

# Removes the lines listed in the profiles file $1, for the state folder named $2.
remove_profiles() {
    [ -f "$1" ] || return 0
    while IFS="$tab" read -r profile_type prior profile; do
        [ -f "$profile" ] && [ ! -L "$profile" ] || continue
        line=$(profile_line "$profile_type" "$2")
        grep -Fqx "$line" "$profile" || continue
        trim=0
        if [ "$(last_byte "$profile")" != 10 ] ||
            { [ "$prior" = no-newline ] && [ "$(tail -n 1 "$profile")" = "$line" ]; }; then
            trim=1
        fi
        edited=$(mktemp "${profile%/*}/.diskuse16x-profile.XXXXXX")
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
    done < "$1"
}

# Removes the directories listed in $1 that are empty now, newest first.
remove_created_dirs() {
    [ -f "$1" ] || return 0
    awk '{ dirs[NR] = $0 } END { for (i = NR; i > 0; i--) print dirs[i] }' "$1" |
        while IFS= read -r directory; do rmdir "$directory" 2>/dev/null || :; done
}

# Removes the du16x link in $1 if it is ours: a symbolic link to diskuse16x.
remove_link() {
    if [ -L "$1/du16x" ] && [ "$(readlink "$1/du16x")" = diskuse16x ]; then rm -f "$1/du16x"; fi
}

legacy_dir=
legacy_path_added=false
# Removes an installation of the space16x command made before 1.0, the way --uninstall
# did: its executable only if it is unchanged, and only the profile lines it added.
remove_legacy() {
    old=${legacy_state:?}
    [ -f "$old/receipt" ] && [ ! -L "$old" ] || return 0
    mkdir "$old/.lock" 2>/dev/null || {
        printf 'Kept the earlier space16x installation, because its installer is active.\n' >&2
        return 0
    }
    for managed in "$old/receipt" "$old/profiles" "$old/dirs" "$old/env.sh" "$old/env.fish"; do
        if [ -L "$managed" ]; then
            rmdir "$old/.lock"
            fail "An earlier installer state file is a symbolic link: $managed"
        fi
    done
    legacy_digest=
    { IFS= read -r legacy_dir && IFS= read -r legacy_digest && IFS= read -r legacy_path_added; } \
        < "$old/receipt" || legacy_dir=
    case "$legacy_dir:$legacy_path_added" in
        /*:true|/*:false) safe_path "$legacy_dir" ;;
        *) legacy_dir= legacy_path_added=false ;;
    esac
    if [ -n "$legacy_dir" ]; then
        legacy_binary=$legacy_dir/space16x
        if [ -f "$legacy_binary" ] && [ ! -L "$legacy_binary" ]; then
            checksum_command
            if [ "$(digest "$legacy_binary")" = "$legacy_digest" ]; then
                rm -f "$legacy_binary"
            else
                printf 'Kept the replacement executable at %s\n' "$legacy_binary" >&2
            fi
        fi
    fi
    remove_profiles "$old/profiles" space16x
    remove_created_dirs "$old/dirs"
    rm -f "$old/receipt" "$old/profiles" "$old/dirs" "$old/env.sh" "$old/env.fish"
    rmdir "$old/.lock"
    rmdir "${old:?}/bin" "${old:?}" 2>/dev/null || :
    printf 'Removed the earlier space16x command and its shell setup.\n' >&2
}

# Prints the path list $2 (by default PATH) without the directory $1.
remove_from_path() {
    remaining=${2-$PATH}:
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
app_id=com.16xbrains.diskuse16x
app_dir=$HOME/Applications
app_path=$app_dir/DiskUse16x.app

# True when $1 is a DiskUse16x app bundle: a real folder with our bundle ID.
is_our_app() {
    [ -d "$1" ] && [ ! -L "$1" ] &&
        [ "$(plutil -extract CFBundleIdentifier raw "$1/Contents/Info.plist" 2>/dev/null)" = "$app_id" ]
}

app_running() {
    pgrep -x DiskUse16x >/dev/null 2>&1
}

# Moves an existing ~/Applications/DiskUse16x.app into the staging folder, which the
# installer removes when it exits. Anything else at that path is left alone.
take_existing_app() {
    is_our_app "$app_path" ||
        fail "Something other than DiskUse16x is at $app_path. Move it, then run the installer again."
    ! app_running || fail "Quit DiskUse16x, then run the installer again."
    mv "$app_path" "$stage/previous.app"
}

safe_path "$legacy_state"
safe_path "$app_dir"
if [ "$action" = uninstall ] && "$app"; then
    [ "$(uname -s)" = Darwin ] || fail "--app is for macOS."
    if [ ! -e "$app_path" ] && [ ! -L "$app_path" ]; then
        printf 'No DiskUse16x app in %s.\n' "$app_dir" >&2
        exit 0
    fi
    stage=$(mktemp -d "${TMPDIR:-/tmp}/diskuse16x-install.XXXXXX")
    take_existing_app
    printf 'Removed %s. Your settings and saved scans stay.\n' "$app_path" >&2
    exit 0
fi
if [ "$action" = uninstall ]; then
    found=false
    if [ -f "$receipt" ]; then
        found=true
        lock_installation
        destination=$installed_dir/diskuse16x
        if [ -f "$destination" ] && [ ! -L "$destination" ]; then
            checksum_command
            if [ "$(digest "$destination")" = "$installed_digest" ]; then
                rm -f "$destination"
            else
                printf 'Kept the replacement executable at %s\n' "$destination" >&2
            fi
        fi
        remove_link "$installed_dir"
        remove_profiles "$profiles" diskuse16x
        remove_created_dirs "$created_dirs"
        rm -f "$receipt" "$profiles" "$created_dirs" "$state/env.sh" "$state/env.fish"
        printf 'Removed the DiskUse16x installation and its shell setup.\n' >&2
    fi
    if [ -f "$legacy_state/receipt" ]; then
        found=true
        remove_legacy
    fi
    "$found" || printf 'No managed DiskUse16x installation was found.\n' >&2
    if "$print_path"; then
        remaining_path=$PATH
        if "$path_added"; then remaining_path=$(remove_from_path "$installed_dir"); fi
        if "$legacy_path_added"; then remaining_path=$(remove_from_path "$legacy_dir" "$remaining_path"); fi
        printf '%s\n' "$remaining_path"
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
if "$app" && [ "$host_os" != Darwin ]; then fail "--app installs the Mac app, so it runs on macOS only."; fi
if [ "$host_os" = Darwin ]; then
    macos_version=$(sw_vers -productVersion) || fail "Could not determine the macOS version."
    major=${macos_version%%.*}
    case "$major" in ''|*[!0-9]*) fail "Could not determine the macOS version." ;; esac
    [ "$major" -ge 14 ] || fail "DiskUse16x requires macOS 14 or later. Found $macos_version."
fi

if command -v curl >/dev/null 2>&1; then
    downloader=curl
elif command -v wget >/dev/null 2>&1 && wget --help 2>&1 | grep -q -- '--max-redirect'; then
    downloader=wget
else
    fail "Install curl or GNU Wget to download the release over HTTPS."
fi

checksum_command
stage=$(mktemp -d "${TMPDIR:-/tmp}/diskuse16x-install.XXXXXX")

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

# Downloads $asset and its .sha256 file from $base into the staging folder, and checks it.
fetch_verified() {
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
}

# Prints the latest release's version, from where GitHub redirects releases/latest.
latest_version() {
    latest_url=https://github.com/$repo/releases/latest
    case "$downloader" in
        curl)
            location=$(curl --disable --silent --show-error --proto '=https' --connect-timeout 15 \
                --max-time 60 --output /dev/null --write-out '%{redirect_url}' "$latest_url") || return 1
            ;;
        wget)
            LC_ALL=C wget --no-config --no-hsts --max-redirect=0 --server-response --spider \
                --timeout=20 "$latest_url" 2> "$stage/wget-latest" || :
            location=$(awk 'tolower($1) == "location:" { sub(/\r$/, "", $2); print $2; exit }' "$stage/wget-latest")
            ;;
    esac
    case "$location" in
        "https://github.com/$repo/releases/tag/v"*) printf '%s\n' "${location##*/releases/tag/v}" ;;
        *) return 1 ;;
    esac
}

if "$app"; then
    if [ -z "$version" ]; then
        version=$(latest_version) || fail "Could not find the latest release of $repo."
        printf '%s\n' "$version" | LC_ALL=C grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9]+([.-][A-Za-z0-9]+)*)?$' ||
            fail "The latest release has an unexpected version."
        case "$version" in *[!A-Za-z0-9.-]*) fail "The latest release has an unexpected version." ;; esac
    fi
    asset=diskuse16x-$version-macos-universal.zip
    base=https://github.com/$repo/releases/download/v$version
    fetch_verified
    ditto -x -k "$stage/$asset" "$stage/app" || fail "Could not expand $asset."
    new_app=$stage/app/DiskUse16x.app
    is_our_app "$new_app" || fail "The release ZIP doesn't contain DiskUse16x.app."
    codesign --verify --strict "$new_app" 2>/dev/null || fail "The app's signature doesn't verify."
    if [ -e "$app_path" ] || [ -L "$app_path" ]; then take_existing_app; fi
    mkdir -p "$app_dir"
    if ! mv "$new_app" "$app_path"; then
        if [ -d "$stage/previous.app" ]; then mv "$stage/previous.app" "$app_path"; fi
        fail "Could not move DiskUse16x.app into $app_dir."
    fi
    printf 'Installed %s %s. Open it from Spotlight, Launchpad or %s.\n' "$app_path" "$version" "$app_dir" >&2
    if [ -d /Applications/DiskUse16x.app ]; then
        printf 'Another copy is in /Applications. Keep one of them, so Spotlight and Full Disk Access find the right one.\n' >&2
    fi
    for old_app in "/Applications/Space 16X.app" "$app_dir/Space 16X.app"; do
        if [ -d "$old_app" ]; then
            printf '%s, the app'"'"'s name before 1.0, is still installed. Move it to the Trash when you no longer need it.\n' "$old_app" >&2
        fi
    done
    if "$print_path"; then printf '%s\n' "$PATH"; fi
    exit 0
fi

asset=diskuse16x-$edition$target.tar.gz
base=https://github.com/$repo/releases/$release
fetch_verified

# Stream only the executable into our staging directory, without unpacking paths.
tar -xOzf "$stage/$asset" diskuse16x > "$stage/diskuse16x" ||
    fail "The release archive does not contain a readable diskuse16x executable."
[ -s "$stage/diskuse16x" ] || fail "The release executable is empty."

lock_installation
# An earlier space16x installation in a folder the user chose keeps that folder.
legacy_choice=
if [ -z "$installed_dir" ] && [ -f "$legacy_state/receipt" ] && [ ! -L "$legacy_state/receipt" ] &&
    IFS= read -r legacy_choice < "$legacy_state/receipt"; then
    case "$legacy_choice" in
        "$legacy_state/bin"|*:*|*"$tab"*) legacy_choice= ;;
        /*) ;;
        *) legacy_choice= ;;
    esac
fi
if [ -z "$bin_dir" ]; then bin_dir=${installed_dir:-${legacy_choice:-$state/bin}}; fi
case "$bin_dir" in /*) ;; *) bin_dir=$PWD/$bin_dir ;; esac
safe_path "$bin_dir"
ensure_directory "$bin_dir"
bin_dir=$(cd "$bin_dir" && pwd -P)
safe_path "$bin_dir"
if [ -n "$installed_dir" ] && [ "$bin_dir" != "$installed_dir" ]; then
    fail "DiskUse16x is installed in $installed_dir. Uninstall it before choosing a new directory."
fi
destination=$bin_dir/diskuse16x
if [ -e "$destination" ] || [ -L "$destination" ]; then
    [ -f "$destination" ] && [ ! -L "$destination" ] &&
        [ "$bin_dir" = "$installed_dir" ] && [ "$(digest "$destination")" = "$installed_digest" ] ||
        fail "An unmanaged file already exists at $destination."
fi
if [ -e "$bin_dir/du16x" ] || [ -L "$bin_dir/du16x" ]; then
    [ -L "$bin_dir/du16x" ] && [ "$(readlink "$bin_dir/du16x")" = diskuse16x ] ||
        fail "An unmanaged file already exists at $bin_dir/du16x."
fi

new_path=$PATH
case ":$PATH:" in
    *":$bin_dir:"*) ;;
    *) new_path=$bin_dir${PATH:+:$PATH}; path_added=true ;;
esac
staged_binary=$(mktemp "$bin_dir/.diskuse16x-install.XXXXXX")
cp "$stage/diskuse16x" "$staged_binary"
chmod 0755 "$staged_binary"
new_digest=$(digest "$staged_binary")
mv -f "$staged_binary" "$destination"
staged_binary=
[ -L "$bin_dir/du16x" ] || ln -s diskuse16x "$bin_dir/du16x"
printf '%s\n%s\n%s\n' "$bin_dir" "$new_digest" "$path_added" > "$receipt"
# The new command is in place, so remove an earlier space16x installation before the
# shell profiles change: a profile only it created then goes, rather than staying empty.
remove_legacy

if "$path_added"; then
    cat > "$state/env.sh" <<'ENV'
if IFS= read -r diskuse16x_bin < "$HOME/.local/share/diskuse16x/receipt"; then
    case ":$PATH:" in
        *":$diskuse16x_bin:"*) ;;
        *) export PATH="$diskuse16x_bin${PATH:+:$PATH}" ;;
    esac
    unset diskuse16x_bin
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
if read -l diskuse16x_bin < "$HOME/.local/share/diskuse16x/receipt"
    if not contains -- "$diskuse16x_bin" $PATH
        set -gx PATH "$diskuse16x_bin" $PATH
    end
end
ENV
            configure_profile fish "${XDG_CONFIG_HOME:-$HOME/.config}/fish/config.fish"
            ;;
        *) configure_profile sh "$HOME/.profile" ;;
    esac
fi
if [ -n "$edition" ]; then
    printf 'Installed DiskUse16x Pro at %s, with du16x beside it\n' "$destination" >&2
else
    printf 'Installed %s, with du16x beside it\n' "$destination" >&2
fi
if "$legacy_path_added" && [ "$legacy_dir" != "$bin_dir" ]; then
    new_path=$(remove_from_path "$legacy_dir" "$new_path")
fi
if "$print_path"; then printf '%s\n' "$new_path"; fi
