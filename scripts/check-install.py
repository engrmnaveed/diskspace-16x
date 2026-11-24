#!/usr/bin/env python3
"""Exercise the release installer with local stand-ins for GitHub downloads."""

import hashlib
import io
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile


INSTALLER = Path(__file__).resolve().with_name("install.sh")
BINARY = b"#!/bin/sh\nprintf 'diskuse16x 0.2.0\\n'\n"
PRO_BINARY = b"#!/bin/sh\nprintf 'diskuse16x 0.2.0 (pro)\\n'\n"
MOCK_DOWNLOAD = """#!/bin/sh
set -eu
if [ "${1:-}" = --help ]; then
    if [ "${TEST_LIGHT_WGET:-0}" = 0 ]; then
        printf '%s\\n' '  --max-redirect number'
    fi
    exit 0
fi
output=
url=
while [ "$#" -gt 0 ]; do
    case "$1" in
        --output) output=$2; shift ;;
        --output-document=*) output=${1#*=} ;;
        http://*|https://*) url=$1 ;;
    esac
    shift
done
printf '%s\\n' "$url" >> "$TEST_ROUTE_LOG"
asset=${url##*/}
case "$url" in
    "$TEST_URL"|"${TEST_URL}.sha256")
        if [ -n "${TEST_REDIRECT_BASE:-}" ]; then
            printf '  HTTP/1.1 302 Found\\n  Location: %s/%s\\n' "$TEST_REDIRECT_BASE" "$asset" >&2
            exit 8
        fi
        ;;
    "${TEST_REDIRECT_BASE:-}"/"$TEST_ASSET"|"${TEST_REDIRECT_BASE:-}"/"$TEST_ASSET.sha256") ;;
    *) printf 'Unexpected URL: %s\\n' "$url" >&2; exit 1 ;;
esac
cp "$TEST_ASSETS/$asset" "$output"
"""


def executable(path, content):
    path.write_text(content, encoding="utf-8")
    path.chmod(0o755)


def setup(root, *, os_name, arch, target, transport, release="latest/download",
          repo="engrmnaveed/diskuse16x", macos_version="14.0"):
    tools = root / "tools"
    downloads = root / "download"
    tools.mkdir()
    downloads.mkdir()
    commands = ("awk", "cat", "chmod", "cp", "grep", "gzip", "ln", "mkdir", "mktemp", "mv",
                "od", "readlink", "rm", "rmdir", "tail", "tar", "tr")
    for name in commands:
        source = shutil.which(name)
        assert source, f"{name} is required for installer tests"
        (tools / name).symlink_to(source)
    # Exercise shasum when available and sha256sum on ordinary Linux systems.
    checksums = ("shasum", "sha256sum") if transport == "wget" else ("sha256sum", "shasum")
    for name in checksums:
        if source := shutil.which(name):
            (tools / name).symlink_to(source)
            break
    else:
        raise AssertionError("No SHA-256 tool available")
    executable(tools / transport, MOCK_DOWNLOAD)
    executable(tools / "uname", f'#!/bin/sh\ncase "$1" in -s) echo {os_name} ;; -m) echo {arch} ;; esac\n')
    if os_name == "Darwin":
        executable(tools / "sw_vers", f"#!/bin/sh\necho {macos_version}\n")

    asset = f"diskuse16x-{target}.tar.gz"
    archive, checksum = release_archive(downloads, asset, BINARY)
    env = os.environ | {
        "PATH": str(tools),
        "HOME": str(root / "home"),
        "SHELL": "/bin/bash",
        "TEST_ASSETS": str(downloads),
        "TEST_ASSET": asset,
        "TEST_ROUTE_LOG": str(root / "requests"),
        "TEST_URL": f"https://github.com/{repo}/releases/{release}/{asset}",
    }
    if transport == "wget":
        env["TEST_REDIRECT_BASE"] = "https://release-assets.example.test"
    return env, archive, checksum


def release_archive(downloads, asset, binary):
    archive = downloads / asset
    with tarfile.open(archive, "w:gz") as stream:
        member = tarfile.TarInfo("diskuse16x")
        member.size = len(binary)
        stream.addfile(member, io.BytesIO(binary))
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    checksum = downloads / f"{asset}.sha256"
    checksum.write_text(f"{digest}  {asset}\n", encoding="utf-8")
    return archive, checksum


def check_editions(platform):
    """--pro installs the Pro archive over Basic, and a plain install switches back."""
    with tempfile.TemporaryDirectory(prefix="diskuse16x-install-qa-") as directory:
        root = Path(directory).resolve()
        env, archive, _ = setup(root, **platform)
        installed = root / "home/.local/share/diskuse16x/bin/diskuse16x"
        result = run(root, env)
        assert result.returncode == 0 and installed.read_bytes() == BINARY, result.stderr

        pro_asset = f"diskuse16x-pro-{platform['target']}.tar.gz"
        pro_env = env | {"TEST_ASSET": pro_asset, "TEST_URL": env["TEST_URL"].replace(archive.name, pro_asset)}
        result = run(root, pro_env, "--pro")
        assert result.returncode == 1 and f"Could not download {pro_asset}" in result.stderr, result.stderr
        assert installed.read_bytes() == BINARY

        release_archive(archive.parent, pro_asset, PRO_BINARY)
        result = run(root, pro_env, "--pro")
        assert result.returncode == 0 and "Installed DiskUse16x Pro" in result.stderr, result.stderr
        assert installed.read_bytes() == PRO_BINARY and os.access(installed, os.X_OK)
        assert pro_asset in Path(env["TEST_ROUTE_LOG"]).read_text(encoding="utf-8")

        result = run(root, env)
        assert result.returncode == 0 and installed.read_bytes() == BINARY, result.stderr
        result = run(root, pro_env, "--pro")
        assert result.returncode == 0 and installed.read_bytes() == PRO_BINARY, result.stderr
        result = run(root, env, "--uninstall")
        assert result.returncode == 0 and not installed.exists(), result.stderr


def run(root, env, *args):
    return subprocess.run(["/bin/sh", INSTALLER, *args], cwd=root, env=env, capture_output=True, text=True)


def accept(root, platform, version=None):
    release = f"download/v{version.lstrip('v')}" if version else "latest/download"
    if version:
        repo = "example/diskuse16x"
        args = ["--repo", repo, "--version", version, "--bin-dir", "path with spaces"]
        bin_dir = root / "path with spaces"
    else:
        repo = "engrmnaveed/diskuse16x"
        args = []
        bin_dir = root / "home/.local/share/diskuse16x/bin"
    env, archive, checksum = setup(root, **platform, release=release, repo=repo)
    result = run(root, env, *args, "--shell-path")
    assert result.returncode == 0, result.stderr
    installed = bin_dir / "diskuse16x"
    assert installed.read_bytes() == BINARY and os.access(installed, os.X_OK)
    assert os.readlink(bin_dir / "du16x") == "diskuse16x"
    assert not list(bin_dir.glob(".diskuse16x-install.*"))
    assert result.stdout.strip() == f"{bin_dir}:{env['PATH']}"
    assert "DiskUse16x installer" in (root / "home/.bashrc").read_text()
    return env, archive, checksum, args, installed


LEGACY_LINE = ('[ ! -r "$HOME/.local/share/space16x/env.sh" ] || '
               '. "$HOME/.local/share/space16x/env.sh" # Space 16X installer')


def legacy_install(home, bin_dir, binary=BINARY):
    """The state that install.sh made for the space16x command before 1.0, with bash."""
    state = home / ".local/share/space16x"
    state.mkdir(parents=True)
    created = [str(state / "bin")] if bin_dir == state / "bin" else []
    bin_dir.mkdir(parents=True, exist_ok=True)
    (bin_dir / "space16x").write_bytes(binary)
    (bin_dir / "space16x").chmod(0o755)
    digest = hashlib.sha256(binary).hexdigest()
    (state / "receipt").write_text(f"{bin_dir}\n{digest}\ntrue\n", encoding="utf-8")
    (state / "dirs").write_text("".join(f"{path}\n" for path in created), encoding="utf-8")
    (state / "env.sh").write_text("# earlier PATH setup\n", encoding="utf-8")
    (home / ".bashrc").write_text(f"# Personal settings\n{LEGACY_LINE}\n", encoding="utf-8")
    (state / "profiles").write_text(f"sh\texisting\t{home / '.bashrc'}\n", encoding="utf-8")
    return state


def check_legacy_migration(platform):
    """Installing over a pre-1.0 space16x installation replaces it and keeps the user's files."""
    # The default folder: the old command and its shell setup go, and the new ones arrive.
    with tempfile.TemporaryDirectory(prefix="diskuse16x-install-qa-") as directory:
        root = Path(directory).resolve()
        env, _, _ = setup(root, **platform)
        home = Path(env["HOME"])
        old_state = legacy_install(home, home / ".local/share/space16x/bin")
        old_bin = old_state / "bin"
        active = env | {"PATH": f"{old_bin}:{env['PATH']}"}
        result = run(root, active, "--shell-path")
        assert result.returncode == 0 and "Removed the earlier space16x" in result.stderr, result.stderr
        new_bin = home / ".local/share/diskuse16x/bin"
        assert (new_bin / "diskuse16x").read_bytes() == BINARY
        assert os.readlink(new_bin / "du16x") == "diskuse16x"
        assert not old_state.exists()
        assert result.stdout.strip() == f"{new_bin}:{env['PATH']}"
        bashrc = (home / ".bashrc").read_text()
        assert LEGACY_LINE not in bashrc and "# DiskUse16x installer" in bashrc
        assert bashrc.startswith("# Personal settings\n")

    # A folder the user chose: the new command goes there, and a replaced executable stays.
    with tempfile.TemporaryDirectory(prefix="diskuse16x-install-qa-") as directory:
        root = Path(directory).resolve()
        env, _, _ = setup(root, **platform)
        home = Path(env["HOME"])
        chosen = root / "my tools"
        old_state = legacy_install(home, chosen)
        (chosen / "space16x").write_bytes(b"a user's replacement")
        result = run(root, env)
        assert result.returncode == 0 and "Kept the replacement" in result.stderr, result.stderr
        assert (chosen / "diskuse16x").read_bytes() == BINARY
        assert os.readlink(chosen / "du16x") == "diskuse16x"
        assert (chosen / "space16x").read_bytes() == b"a user's replacement"
        assert not old_state.exists()

    # --uninstall removes a pre-1.0 installation on its own too.
    with tempfile.TemporaryDirectory(prefix="diskuse16x-install-qa-") as directory:
        root = Path(directory).resolve()
        env, _, _ = setup(root, **platform)
        home = Path(env["HOME"])
        old_state = legacy_install(home, home / ".local/share/space16x/bin")
        active = env | {"PATH": f"{old_state / 'bin'}:{env['PATH']}"}
        result = run(root, active, "--uninstall", "--shell-path")
        assert result.returncode == 0 and "Removed the earlier space16x" in result.stderr, result.stderr
        assert result.stdout.strip() == env["PATH"]
        assert not old_state.exists()
        assert (home / ".bashrc").read_text() == "# Personal settings\n"

    # A file named du16x that the installer didn't make is never replaced.
    with tempfile.TemporaryDirectory(prefix="diskuse16x-install-qa-") as directory:
        root = Path(directory).resolve()
        env, _, _ = setup(root, **platform)
        theirs = root / "bin"
        theirs.mkdir()
        (theirs / "du16x").write_bytes(b"another tool")
        result = run(root, env, "--bin-dir", str(theirs))
        assert result.returncode == 1 and "unmanaged file already exists" in result.stderr, result.stderr
        assert (theirs / "du16x").read_bytes() == b"another tool"


def main():
    platforms = (
        dict(os_name="Darwin", arch="arm64", target="aarch64-apple-darwin", transport="curl"),
        dict(os_name="Darwin", arch="x86_64", target="x86_64-apple-darwin", transport="curl"),
        dict(os_name="Linux", arch="aarch64", target="aarch64-unknown-linux-musl", transport="wget"),
        dict(os_name="Linux", arch="x86_64", target="x86_64-unknown-linux-musl", transport="curl"),
    )
    for platform in platforms:
        with tempfile.TemporaryDirectory(prefix="diskuse16x-install-qa-") as directory:
            root = Path(directory).resolve()
            version = "v0.2.0" if platform["transport"] == "wget" else None
            env, archive, checksum, args, installed = accept(root, platform, version)

            original_checksum = checksum.read_text(encoding="utf-8")
            checksum.write_text(f"{'0' * 64}  {archive.name}\n", encoding="utf-8")
            result = run(root, env, *args)
            assert result.returncode == 1 and "checksum mismatch" in result.stderr
            assert installed.read_bytes() == BINARY
            assert not list(installed.parent.glob(".diskuse16x-install.*"))

            checksum.write_text(original_checksum, encoding="utf-8")
            if platform["transport"] == "wget":
                unsafe = env | {"TEST_REDIRECT_BASE": "http://release-assets.example.test"}
                result = run(root, unsafe, *args)
                assert result.returncode == 1 and "Could not download" in result.stderr
                assert "http://" not in Path(env["TEST_ROUTE_LOG"]).read_text(encoding="utf-8")

                result = run(root, env | {"TEST_LIGHT_WGET": "1"}, *args)
                assert result.returncode == 1 and "Install curl or GNU Wget" in result.stderr

            archive.unlink()
            result = run(root, env, *args)
            assert result.returncode == 1 and "Could not download" in result.stderr
            assert installed.read_bytes() == BINARY

            result = run(root, env, "--repo", "owner/repo/extra")
            assert result.returncode == 1 and "Invalid GitHub repository" in result.stderr

            active = env | {"PATH": f"{installed.parent}:{env['PATH']}"}
            result = run(root, active, "--uninstall", "--shell-path")
            assert result.returncode == 0, result.stderr
            assert result.stdout.strip() == env["PATH"]
            assert not installed.exists() and not (installed.parent / "du16x").is_symlink()
            assert not (root / "home/.bashrc").exists()
            assert not (root / "home/.profile").exists()
            assert not (root / "home/.local/share/diskuse16x").exists()

    for platform in (platforms[0], platforms[3]):
        check_editions(platform)
        check_legacy_migration(platform)

    with tempfile.TemporaryDirectory(prefix="diskuse16x-install-qa-") as directory:
        root = Path(directory).resolve()
        env, _, _ = setup(root, **platforms[0])
        home = Path(env["HOME"])
        home.mkdir()
        profile = home / "shell-settings"
        original = b"# Personal settings\nexport PERSONAL=yes"
        profile.write_bytes(original)
        profile.chmod(0o640)
        (home / ".zshrc").symlink_to(profile.name)
        env["SHELL"] = "/bin/zsh"
        result = run(root, env, "--shell-path")
        assert result.returncode == 0, result.stderr
        active = env | {"PATH": result.stdout.strip()}
        result = run(root, active, "--shell-path")
        assert result.returncode == 0, result.stderr
        assert profile.read_text().count("# DiskUse16x installer") == 1
        binary = home / ".local/share/diskuse16x/bin/diskuse16x"
        binary.write_bytes(b"a user's replacement")
        result = run(root, active, "--uninstall", "--shell-path")
        assert result.returncode == 0 and "Kept the replacement" in result.stderr
        assert result.stdout.strip() == env["PATH"]
        assert (home / ".zshrc").is_symlink()
        assert profile.read_bytes() == original and profile.stat().st_mode & 0o777 == 0o640
        assert binary.read_bytes() == b"a user's replacement"

    with tempfile.TemporaryDirectory(prefix="diskuse16x-install-qa-") as directory:
        root = Path(directory).resolve()
        unsupported = platforms[0] | {"arch": "riscv64"}
        env, _, _ = setup(root, **unsupported)
        result = run(root, env, "--repo", "example/diskuse16x")
        assert result.returncode == 1 and "No prebuilt CLI" in result.stderr

    with tempfile.TemporaryDirectory(prefix="diskuse16x-install-qa-") as directory:
        root = Path(directory).resolve()
        env, _, _ = setup(root, **platforms[0], macos_version="13.7.6")
        result = run(root, env)
        assert result.returncode == 1 and "requires macOS 14" in result.stderr
        assert not (root / "home").exists()

    print("Installer acceptance checks passed.")


if __name__ == "__main__":
    main()
