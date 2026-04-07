"""jdwp-mcp: Debug live JVMs through JDWP from any MCP-compatible agent."""

import os
import platform
import subprocess
import sys
import tarfile
import zipfile
from io import BytesIO
from pathlib import Path
from urllib.request import urlopen
import json

REPO = "navicore/jdwp-mcp"
BINARY_NAME = "jdwp-mcp"


def _get_platform_target():
    system = platform.system().lower()
    machine = platform.machine().lower()

    if system == "linux":
        os_name = "linux"
    elif system == "darwin":
        os_name = "macos"
    elif system == "windows":
        os_name = "windows"
    else:
        raise RuntimeError(f"Unsupported OS: {system}")

    if machine in ("x86_64", "amd64"):
        arch = "x86_64"
    elif machine in ("aarch64", "arm64"):
        arch = "aarch64"
    else:
        raise RuntimeError(f"Unsupported architecture: {machine}")

    return f"{os_name}-{arch}"


def _get_binary_path():
    return Path(__file__).parent / BINARY_NAME


def _ensure_binary():
    binary = _get_binary_path()
    if binary.exists():
        return binary

    target = _get_platform_target()

    # Fetch latest release
    with urlopen(f"https://api.github.com/repos/{REPO}/releases/latest") as resp:
        release = json.loads(resp.read())

    tag = release["tag_name"]
    ext = "zip" if "windows" in target else "tar.gz"
    asset_name = f"jdwp-mcp-{target}.{ext}"

    asset_url = None
    for asset in release["assets"]:
        if asset["name"] == asset_name:
            asset_url = asset["browser_download_url"]
            break

    if not asset_url:
        raise RuntimeError(
            f"No release asset found for {target}. "
            f"Install from source: cargo install --git https://github.com/{REPO}"
        )

    print(f"Downloading jdwp-mcp {tag} ({target})...", file=sys.stderr)

    with urlopen(asset_url) as resp:
        data = resp.read()

    if ext == "zip":
        with zipfile.ZipFile(BytesIO(data)) as zf:
            for name in zf.namelist():
                if BINARY_NAME in name:
                    binary.write_bytes(zf.read(name))
                    break
    else:
        with tarfile.open(fileobj=BytesIO(data), mode="r:gz") as tf:
            for member in tf.getmembers():
                if BINARY_NAME in member.name:
                    f = tf.extractfile(member)
                    if f:
                        binary.write_bytes(f.read())
                    break

    binary.chmod(0o755)
    print(f"Installed to {binary}", file=sys.stderr)
    return binary


def main():
    binary = _ensure_binary()
    os.execv(str(binary), [str(binary)] + sys.argv[1:])
