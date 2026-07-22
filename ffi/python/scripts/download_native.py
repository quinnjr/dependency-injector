#!/usr/bin/env python3
"""
Download pre-built native library for the current platform.

This script downloads the appropriate pre-built native library from GitHub releases.
It can be run manually or as part of a post-install hook.

Usage:
    python -m scripts.download_native

Environment variables:
    DI_LIBRARY_PATH: Skip download and use this path instead
    DI_SKIP_DOWNLOAD: Skip download entirely
    DI_GITHUB_TOKEN: GitHub token for rate limiting
"""

from __future__ import annotations

import hashlib
import os
import platform
import re
import sys
import urllib.request
import json
from pathlib import Path

# Configuration
REPO_OWNER = "pegasusheavy"
REPO_NAME = "dependency-injector"
PACKAGE_DIR = Path(__file__).parent.parent / "dependency_injector"


def get_version() -> str:
    """Get package version from __init__.py."""
    init_file = PACKAGE_DIR / "__init__.py"
    with open(init_file) as f:
        for line in f:
            if line.startswith("__version__"):
                return line.split("=")[1].strip().strip('"\'')
    return "0.0.0"


def get_platform_info() -> tuple[str, str, str]:
    """Get platform, architecture, and library name.

    Returns:
        Tuple of (platform_tag, asset_name, library_name)
    """
    system = platform.system().lower()
    machine = platform.machine().lower()

    # Normalize machine architecture
    if machine in ("x86_64", "amd64"):
        arch = "x64"
    elif machine in ("aarch64", "arm64"):
        arch = "arm64"
    else:
        arch = machine

    # Map to asset names
    if system == "linux":
        lib_name = "libdependency_injector.so"
        asset_name = f"libdependency_injector-linux-{arch}.so"
    elif system == "darwin":
        lib_name = "libdependency_injector.dylib"
        asset_name = f"libdependency_injector-darwin-{arch}.dylib"
    elif system == "windows":
        lib_name = "dependency_injector.dll"
        asset_name = f"dependency_injector-win32-{arch}.dll"
    else:
        raise RuntimeError(f"Unsupported platform: {system}")

    return f"{system}-{arch}", asset_name, lib_name


def get_release_assets(version: str) -> tuple[str, list[dict] | None]:
    """Fetch release metadata from GitHub.

    Returns:
        Tuple of (resolved tag, asset list), or (tag, None) if the release
        info could not be fetched.
    """
    tag = version if version.startswith("v") else f"v{version}"
    api_url = f"https://api.github.com/repos/{REPO_OWNER}/{REPO_NAME}/releases/tags/{tag}"

    headers = {
        "Accept": "application/vnd.github.v3+json",
        "User-Agent": f"dependency-injector-python/{version}",
    }

    if token := os.environ.get("DI_GITHUB_TOKEN"):
        headers["Authorization"] = f"token {token}"

    try:
        req = urllib.request.Request(api_url, headers=headers)
        with urllib.request.urlopen(req, timeout=30) as response:
            release = json.loads(response.read().decode())
        return tag, release.get("assets", [])
    except Exception as e:
        print(f"Warning: Could not fetch release info: {e}", file=sys.stderr)

    return tag, None


def fetch_text(url: str) -> str:
    """Download a URL body as text (used for the SHA256SUMS asset)."""
    headers = {"User-Agent": f"dependency-injector-python/{get_version()}"}
    req = urllib.request.Request(url, headers=headers)
    with urllib.request.urlopen(req, timeout=30) as response:
        return response.read().decode("utf-8")


def parse_sha256sums(text: str, asset_name: str) -> str | None:
    """Parse sha256sum output and return the expected hash for asset_name.

    Lines look like "<hex>  <filename>" (two spaces), or "<hex> *<filename>"
    in binary mode. Returns None if the file has no entry for asset_name.
    """
    for raw_line in text.splitlines():
        match = re.match(r"^([0-9a-fA-F]{64})\s+\*?(.+)$", raw_line.strip())
        if match and match.group(2) == asset_name:
            return match.group(1).lower()
    return None


def sha256_file(path: Path) -> str:
    """Compute the SHA256 hex digest of a file."""
    digest = hashlib.sha256()
    with open(path, "rb") as f:
        while chunk := f.read(65536):
            digest.update(chunk)
    return digest.hexdigest()


def verify_checksum(assets: list[dict], tag: str, asset_name: str, lib_path: Path) -> bool:
    """Verify the downloaded file against the release's SHA256SUMS asset.

    - Release has no SHA256SUMS asset (pre-checksum release): warn loudly
      and return True (proceed).
    - SHA256SUMS exists but cannot be fetched, has no entry for the asset,
      or the hash mismatches: delete the downloaded file and return False.
    """
    sums_asset = next((a for a in assets if a["name"] == "SHA256SUMS"), None)

    if sums_asset is None:
        print(
            f"⚠️  WARNING: release {tag} has no SHA256SUMS asset; "
            "skipping checksum verification (pre-checksum release)."
        )
        return True

    try:
        sums_text = fetch_text(sums_asset["browser_download_url"])
    except Exception as e:
        lib_path.unlink(missing_ok=True)
        print(f"❌ Failed to download SHA256SUMS from release {tag}: {e}")
        print("   Deleted the downloaded library since it could not be verified.")
        return False

    expected = parse_sha256sums(sums_text, asset_name)
    if expected is None:
        lib_path.unlink(missing_ok=True)
        print(f"❌ SHA256SUMS in release {tag} has no entry for '{asset_name}'. Deleted the downloaded library.")
        return False

    actual = sha256_file(lib_path)
    if actual != expected:
        lib_path.unlink(missing_ok=True)
        print(f"❌ Checksum mismatch for '{asset_name}' (release {tag}):")
        print(f"   expected {expected}")
        print(f"   actual   {actual}")
        print("   The downloaded library has been deleted. This may indicate a corrupted or tampered download.")
        return False

    print(f"✅ SHA256 checksum verified: {actual}")
    return True


def download_file(url: str, dest: Path) -> None:
    """Download a file from URL to destination."""
    print(f"Downloading from {url}...")

    headers = {"User-Agent": f"dependency-injector-python/{get_version()}"}
    req = urllib.request.Request(url, headers=headers)

    with urllib.request.urlopen(req, timeout=60) as response:
        dest.parent.mkdir(parents=True, exist_ok=True)
        with open(dest, "wb") as f:
            while chunk := response.read(8192):
                f.write(chunk)

    # Make executable on Unix
    if sys.platform != "win32":
        dest.chmod(0o755)


def main() -> int:
    """Main entry point."""
    print("📦 dependency-injector: Checking native library...")

    # Check for skip flag
    if os.environ.get("DI_SKIP_DOWNLOAD"):
        print("⏭️  DI_SKIP_DOWNLOAD set, skipping download")
        return 0

    # Check for custom library path
    if custom_path := os.environ.get("DI_LIBRARY_PATH"):
        if Path(custom_path).exists():
            print(f"✅ Using custom library: {custom_path}")
            return 0
        print(f"⚠️  DI_LIBRARY_PATH set but file not found: {custom_path}")

    # Get platform info
    try:
        platform_tag, asset_name, lib_name = get_platform_info()
    except RuntimeError as e:
        print(f"❌ {e}")
        return 1

    native_dir = PACKAGE_DIR / "native"
    lib_path = native_dir / lib_name

    # Check if already exists
    if lib_path.exists():
        print(f"✅ Native library already exists: {lib_path}")
        return 0

    # Check for local build
    for parent_levels in range(3, 6):
        local_build = PACKAGE_DIR
        for _ in range(parent_levels):
            local_build = local_build.parent
        local_build = local_build / "target" / "release" / lib_name
        if local_build.exists():
            print(f"✅ Found local build: {local_build}")
            return 0

    # Download from GitHub
    version = get_version()
    tag, assets = get_release_assets(version)
    print(f"📥 Downloading asset '{asset_name}' from release tag '{tag}' for {platform_tag}...")

    download_url = None
    if assets is not None:
        download_url = next(
            (a["browser_download_url"] for a in assets if a["name"] == asset_name), None
        )

    if not download_url:
        print(f"❌ Could not find asset '{asset_name}' in release {tag}")
        print()
        print("You can:")
        print("  1. Build locally: cargo rustc --release --features ffi --crate-type cdylib")
        print("  2. Set DI_LIBRARY_PATH to point to an existing library")
        print("  3. Install a platform-specific wheel instead of sdist")
        return 1

    try:
        download_file(download_url, lib_path)
    except Exception as e:
        print(f"❌ Download failed: {e}")
        print()
        print("You can:")
        print("  1. Build locally: cargo rustc --release --features ffi --crate-type cdylib")
        print("  2. Set DI_LIBRARY_PATH to point to an existing library")
        return 1

    # Verify against the release's SHA256SUMS (deletes the file on failure)
    if not verify_checksum(assets, tag, asset_name, lib_path):
        return 1

    print(f"✅ Downloaded to: {lib_path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

