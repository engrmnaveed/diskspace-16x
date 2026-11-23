#!/usr/bin/env python3
"""Install, switch and remove the .deb or .rpm packages that scripts/package-linux.sh made.

  check-linux-packages.py deb DIST   on Debian or Ubuntu, as root or with sudo
  check-linux-packages.py rpm DIST   in a Fedora container, through Docker

Each check installs Basic, switches to Pro, switches back and removes the package,
running the installed diskuse16x after each step.
"""

import hashlib
import os
from pathlib import Path
import platform
import shlex
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent
VERSION = (ROOT / "scripts" / "release-version").read_text().strip()
FEDORA = "registry.fedoraproject.org/fedora:latest"

# Runs inside the system under test, as root, with the packages in $DIST.
STEPS = r"""
set -eu
check() {
    want="diskuse16x $VERSION$2"
    got=$(/usr/bin/diskuse16x --version)
    [ "$got" = "$want" ] || { echo "Expected '$want', got '$got'." >&2; exit 1; }
    [ "$(/usr/bin/du16x --version)" = "$want" ] || { echo "du16x doesn't run diskuse16x." >&2; exit 1; }
    has_file "$1" "/usr/share/doc/$1/cli.md"
    folder=$(mktemp -d)
    echo 'DiskUse16x' > "$folder/file"
    /usr/bin/diskuse16x scan "$folder" --json | grep -q "\"edition\": *\"$3\""
    rm -rf "$folder"
    echo "ok: $1 installed and runs"
}
install_basic; check diskuse16x '' basic
switch_to_pro; check diskuse16x-pro ' (pro)' pro
has_file diskuse16x-pro /usr/share/doc/diskuse16x-pro/cli-pro.md
installed diskuse16x && { echo 'Installing Pro left Basic installed.' >&2; exit 1; }
switch_to_basic; check diskuse16x '' basic
installed diskuse16x-pro && { echo 'Installing Basic left Pro installed.' >&2; exit 1; }
remove_basic
test ! -e /usr/bin/diskuse16x && test ! -L /usr/bin/du16x
echo 'ok: removed'
"""

DEB = r"""
installed() { dpkg-query -W -f '${db:Status-Status}' "$1" 2>/dev/null | grep -qx installed; }
# Container images often leave out /usr/share/doc, so ask the package database.
has_file() { dpkg -L "$1" | grep -qx "$2"; }
install_basic() { dpkg -i "$DIST/diskuse16x_$ARCH.deb"; }
# Conflicts and Replaces let dpkg remove the other edition by itself.
switch_to_pro() { dpkg -i "$DIST/diskuse16x-pro_$ARCH.deb"; }
switch_to_basic() { dpkg -i "$DIST/diskuse16x_$ARCH.deb"; }
remove_basic() { dpkg -r diskuse16x; }
"""

# The packages are unsigned. RPM 6 (Fedora 43 and later) refuses unsigned packages
# in plain rpm -i, so the check uses dnf, as docs/install.md tells users to.
RPM = r"""
installed() { rpm -q "$1" >/dev/null 2>&1; }
has_file() { rpm -ql "$1" | grep -qx "$2"; }
dnf_local() { dnf -y -q --disablerepo='*' "$@"; }
install_basic() {
    rpm -K --nosignature "$DIST"/*.rpm
    dnf_local install "$DIST/diskuse16x.$ARCH.rpm"
}
switch_to_pro() {
    # The editions conflict, so switching needs --allowerasing.
    if dnf_local install "$DIST/diskuse16x-pro.$ARCH.rpm" >/dev/null 2>&1; then
        echo 'dnf installed Pro beside Basic.' >&2
        exit 1
    fi
    dnf_local install --allowerasing "$DIST/diskuse16x-pro.$ARCH.rpm"
}
switch_to_basic() { dnf_local install --allowerasing "$DIST/diskuse16x.$ARCH.rpm"; }
remove_basic() { dnf_local remove diskuse16x; }
"""


def verify_checksums(dist, names):
    for name in names:
        expected = (dist / f"{name}.sha256").read_text().split()
        actual = hashlib.sha256((dist / name).read_bytes()).hexdigest()
        if expected != [actual, name]:
            sys.exit(f"{name}.sha256 does not match {name}.")


def main():
    if len(sys.argv) != 3 or sys.argv[1] not in ("deb", "rpm"):
        sys.exit(__doc__)
    kind, dist = sys.argv[1], Path(sys.argv[2]).resolve()
    machine = platform.machine().lower()
    rpm_arch = {"x86_64": "x86_64", "amd64": "x86_64", "aarch64": "aarch64", "arm64": "aarch64"}[machine]
    if kind == "deb":
        arch = {"x86_64": "amd64", "aarch64": "arm64"}[rpm_arch]
        names = [f"diskuse16x_{arch}.deb", f"diskuse16x-pro_{arch}.deb"]
    else:
        arch = rpm_arch
        names = [f"diskuse16x.{arch}.rpm", f"diskuse16x-pro.{arch}.rpm"]
    verify_checksums(dist, names)

    if kind == "deb":
        script = DEB + STEPS
        command = ["sh", "-c", script]
        if os.geteuid() != 0:
            command = ["sudo", "--preserve-env=DIST,ARCH,VERSION"] + command
        env = dict(os.environ, DIST=str(dist), ARCH=arch, VERSION=VERSION)
    else:
        script = RPM + STEPS
        command = [
            "docker", "run", "--rm", "-v", f"{dist}:/dist:ro",
            "-e", "DIST=/dist", "-e", f"ARCH={arch}", "-e", f"VERSION={VERSION}",
            FEDORA, "sh", "-c", script,
        ]
        env = None
    print("+", shlex.join(command[:-1]), "<steps>", flush=True)
    subprocess.run(command, check=True, env=env)
    print(f"{kind} packages for {arch}: install, switch and remove work.")


if __name__ == "__main__":
    main()
