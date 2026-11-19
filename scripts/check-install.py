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
BINARY = b"#!/bin/sh\nprintf 'space16x 0.2.0\\n'\n"
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
          repo="engrmnaveed/diskspace-16x", macos_version="14.0"):
    tools = root / "tools"
    downloads = root / "download"
    tools.mkdir()
    downloads.mkdir()
    commands = ("awk", "cat", "chmod", "cp", "grep", "gzip", "mkdir", "mktemp", "mv",
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

    asset = f"space16x-{target}.tar.gz"
    archive = downloads / asset
    with tarfile.open(archive, "w:gz") as stream:
        member = tarfile.TarInfo("space16x")
        member.size = len(BINARY)
        stream.addfile(member, io.BytesIO(BINARY))
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    checksum = downloads / f"{asset}.sha256"
    checksum.write_text(f"{digest}  {asset}\n", encoding="utf-8")
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


def run(root, env, *args):
    return subprocess.run(["/bin/sh", INSTALLER, *args], cwd=root, env=env, capture_output=True, text=True)


def accept(root, platform, version=None):
    release = f"download/v{version.lstrip('v')}" if version else "latest/download"
    if version:
        repo = "example/space16x"
        args = ["--repo", repo, "--version", version, "--bin-dir", "path with spaces"]
        bin_dir = root / "path with spaces"
    else:
        repo = "engrmnaveed/diskspace-16x"
        args = []
        bin_dir = root / "home/.local/share/space16x/bin"
    env, archive, checksum = setup(root, **platform, release=release, repo=repo)
    result = run(root, env, *args, "--shell-path")
    assert result.returncode == 0, result.stderr
    installed = bin_dir / "space16x"
    assert installed.read_bytes() == BINARY and os.access(installed, os.X_OK)
    assert not list(bin_dir.glob(".space16x-install.*"))
    assert result.stdout.strip() == f"{bin_dir}:{env['PATH']}"
    assert "Space 16X installer" in (root / "home/.bashrc").read_text()
    return env, archive, checksum, args, installed


def main():
    platforms = (
        dict(os_name="Darwin", arch="arm64", target="aarch64-apple-darwin", transport="curl"),
        dict(os_name="Darwin", arch="x86_64", target="x86_64-apple-darwin", transport="curl"),
        dict(os_name="Linux", arch="aarch64", target="aarch64-unknown-linux-musl", transport="wget"),
        dict(os_name="Linux", arch="x86_64", target="x86_64-unknown-linux-musl", transport="curl"),
    )
    for platform in platforms:
        with tempfile.TemporaryDirectory(prefix="space16x-install-qa-") as directory:
            root = Path(directory).resolve()
            version = "v0.2.0" if platform["transport"] == "wget" else None
            env, archive, checksum, args, installed = accept(root, platform, version)

            original_checksum = checksum.read_text(encoding="utf-8")
            checksum.write_text(f"{'0' * 64}  {archive.name}\n", encoding="utf-8")
            result = run(root, env, *args)
            assert result.returncode == 1 and "checksum mismatch" in result.stderr
            assert installed.read_bytes() == BINARY
            assert not list(installed.parent.glob(".space16x-install.*"))

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
            assert not installed.exists()
            assert not (root / "home/.bashrc").exists()
            assert not (root / "home/.profile").exists()
            assert not (root / "home/.local/share/space16x").exists()

    with tempfile.TemporaryDirectory(prefix="space16x-install-qa-") as directory:
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
        assert profile.read_text().count("# Space 16X installer") == 1
        binary = home / ".local/share/space16x/bin/space16x"
        binary.write_bytes(b"a user's replacement")
        result = run(root, active, "--uninstall", "--shell-path")
        assert result.returncode == 0 and "Kept the replacement" in result.stderr
        assert result.stdout.strip() == env["PATH"]
        assert (home / ".zshrc").is_symlink()
        assert profile.read_bytes() == original and profile.stat().st_mode & 0o777 == 0o640
        assert binary.read_bytes() == b"a user's replacement"

    with tempfile.TemporaryDirectory(prefix="space16x-install-qa-") as directory:
        root = Path(directory).resolve()
        unsupported = platforms[0] | {"arch": "riscv64"}
        env, _, _ = setup(root, **unsupported)
        result = run(root, env, "--repo", "example/space16x")
        assert result.returncode == 1 and "No prebuilt CLI" in result.stderr

    with tempfile.TemporaryDirectory(prefix="space16x-install-qa-") as directory:
        root = Path(directory).resolve()
        env, _, _ = setup(root, **platforms[0], macos_version="13.7.6")
        result = run(root, env)
        assert result.returncode == 1 and "requires macOS 14" in result.stderr
        assert not (root / "home").exists()

    print("Installer acceptance checks passed.")


if __name__ == "__main__":
    main()
