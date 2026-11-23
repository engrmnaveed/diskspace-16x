#!/bin/sh
# Build a .deb and an .rpm from a static Linux CLI archive made by
# scripts/package-cli.sh, for the archive's own edition and CPU:
#   dist/space16x-TARGET.tar.gz      -> space16x_ARCH.deb, space16x.ARCH.rpm
#   dist/space16x-pro-TARGET.tar.gz  -> space16x-pro_ARCH.deb, space16x-pro.ARCH.rpm
# The names carry no version, so releases/latest/download/NAME always works.
# Needs dpkg-deb and rpmbuild (apt-get install rpm on Debian or Ubuntu).
set -eu

[ "$#" -eq 1 ] || { echo "Usage: $0 dist/space16x[-pro]-TARGET.tar.gz" >&2; exit 1; }
archive=$1
cd "$(dirname "$0")/.."
project="$PWD"
case "$archive" in /*) ;; *) archive="$project/$archive" ;; esac

case "${archive##*/}" in
    space16x-pro-*) name=space16x-pro other=space16x edition=pro ;;
    space16x-*) name=space16x other=space16x-pro edition=basic ;;
    *) echo "Expected a space16x CLI archive." >&2; exit 1 ;;
esac
case "${archive##*/}" in
    *-x86_64-unknown-linux-musl.tar.gz) deb_arch=amd64 rpm_arch=x86_64 ;;
    *-aarch64-unknown-linux-musl.tar.gz) deb_arch=arm64 rpm_arch=aarch64 ;;
    *) echo "Expected a Linux musl archive." >&2; exit 1 ;;
esac

version="$(cat scripts/release-version)"
# Both formats sort 1.0.0~rc1 before 1.0.0.
package_version=$(printf '%s' "$version" | sed 's/-/~/g')
expected="space16x $version"
[ "$edition" = basic ] || expected="$expected (pro)"
homepage=https://github.com/engrmnaveed/diskspace-16x
summary="Disk space explorer for the terminal"
description="Space 16X shows what uses the space on a disk or in a folder, in an
interactive explorer or as a JSON report. Its scans read file metadata only,
and it changes nothing except a Move to Trash you confirm."
if [ "$edition" = pro ]; then
    summary="$summary, Pro edition"
    description="$description
This is the Pro edition, which adds scan comparison, history, cleanup
suggestions, a duplicate finder that reads files only when you run it, and
more."
fi

mkdir -p dist
stage="$(mktemp -d "$project/dist/.space16x-linux.XXXXXX")"
trap 'rm -rf "$stage"' 0
payload="$stage/payload"
mkdir "$payload"
tar -xzf "$archive" -C "$payload"
binary="$payload/space16x"
if [ "$("$binary" --version)" != "$expected" ]; then
    echo "The archive's CLI version does not match scripts/release-version." >&2
    exit 1
fi
set -- cli.md example-report.json
[ "$edition" = basic ] || set -- "$@" cli-pro.md

# Debian package.
deb="$stage/deb"
doc="$deb/usr/share/doc/$name"
mkdir -p "$deb/DEBIAN" "$deb/usr/bin" "$doc"
install -m 0755 "$binary" "$deb/usr/bin/space16x"
for file in "$@"; do install -m 0644 "$payload/$file" "$doc/$file"; done
install -m 0644 "$payload/LICENSE" "$doc/copyright"
installed_size=$(du -sk "$deb/usr" | cut -f1)
{
    printf 'Package: %s\nVersion: %s\nArchitecture: %s\n' "$name" "$package_version" "$deb_arch"
    printf 'Maintainer: 16X Brains <%s>\n' "$homepage"
    printf 'Installed-Size: %s\nSection: utils\nPriority: optional\n' "$installed_size"
    # Both editions install /usr/bin/space16x, so installing one replaces the other.
    printf 'Conflicts: %s\nReplaces: %s\n' "$other" "$other"
    printf 'Homepage: %s\nDescription: %s\n' "$homepage" "$summary"
    printf '%s\n' "$description" | sed 's/^/ /'
} > "$deb/DEBIAN/control"
deb_file="${name}_$deb_arch.deb"
dpkg-deb --root-owner-group -Zxz --build "$deb" "$stage/$deb_file" >&2

# RPM package.
rpm="$stage/rpm"
mkdir -p "$rpm/SPECS" "$rpm/SOURCES" "$rpm/RPMS"
cp "$binary" "$payload/LICENSE" "$rpm/SOURCES/"
for file in "$@"; do cp "$payload/$file" "$rpm/SOURCES/"; done
cat > "$rpm/SPECS/$name.spec" <<EOF
Name: $name
Version: $package_version
Release: 1
Summary: $summary
License: MIT
URL: $homepage
# Both editions install /usr/bin/space16x.
Conflicts: $other
AutoReqProv: no
%global debug_package %{nil}
%global __os_install_post %{nil}
%define _build_id_links none
%define _binary_payload w9.xzdio

%description
$description

%install
install -D -m 0755 %{_sourcedir}/space16x %{buildroot}%{_bindir}/space16x
install -D -m 0644 %{_sourcedir}/LICENSE %{buildroot}%{_datadir}/licenses/%{name}/LICENSE
install -d %{buildroot}%{_docdir}/%{name}
for file in $*; do install -m 0644 %{_sourcedir}/\$file %{buildroot}%{_docdir}/%{name}/; done

%files
%{_bindir}/space16x
%doc %{_docdir}/%{name}
%license %{_datadir}/licenses/%{name}
EOF
rpmbuild --quiet -bb --target "$rpm_arch" \
    --define "_topdir $rpm" --define "_rpmdir $rpm/RPMS" \
    --define "_build_name_fmt %%{NAME}.%%{ARCH}.rpm" \
    "$rpm/SPECS/$name.spec" >&2
rpm_file="$name.$rpm_arch.rpm"
mv "$rpm/RPMS/$rpm_file" "$stage/$rpm_file"

for file in "$deb_file" "$rpm_file"; do
    (cd "$stage" && sha256sum "$file") > "$stage/$file.sha256"
    mv -f "$stage/$file" "$stage/$file.sha256" dist/
    printf '%s\n' "$project/dist/$file"
done
