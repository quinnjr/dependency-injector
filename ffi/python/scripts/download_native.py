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
    DI_GITHUB_TOKEN: GitHub token for rate limiting. Only ever sent to
        api.github.com — never to download hosts or arbitrary redirect
        targets (see the credential invariant below).
    DI_REQUIRE_CHECKSUM: If set (non-empty), hard-fail when the release has
        no SHA256SUMS asset instead of proceeding with a warning

Credential invariant:

The Authorization header carrying DI_GITHUB_TOKEN is attached only to the
api.github.com release-metadata request. Asset and SHA256SUMS downloads never
carry it. Because CPython's default urllib redirect handler copies every
request header (except content-length/content-type) onto the redirect target
with no cross-origin check — and permits an https -> http downgrade — all
requests go through an opener installed with `_SafeRedirectHandler`, which
strips Authorization whenever the redirect target host is not api.github.com
and refuses to follow non-https redirects. Redirects are capped at
MAX_REDIRECTS (5), matching the Node installer; urllib's default cap is 10.

Failure policy (aligned with the Node installer, ffi/nodejs/scripts/install.js):

- Network/download failure (release metadata fetch failure, missing platform
  asset on the release, a truncated download, or the library download itself
  failing): warn and exit 0. This is non-fatal because platform wheels
  normally bundle the library, and users can build it from source.
- Checksum mismatch, missing SHA256SUMS entry for the asset, or failure to
  fetch an existing SHA256SUMS asset: hard fail (exit 1). The downloaded
  file is deleted.
- Release has no SHA256SUMS asset at all (pre-checksum release): warn and
  proceed, unless DI_REQUIRE_CHECKSUM is set (non-empty), in which case
  hard fail (exit 1).
- Unsupported platform: hard fail (exit 1).

A truncated transfer (fewer bytes than the advertised Content-Length) is
reported as DownloadIncompleteError, a network error — not as a checksum
failure — so an interrupted download is never described as tampering.

The library is downloaded to a temporary file next to the final path and only
moved into place (atomic Path.replace) after the checksum verifies and the
file mode is set, so the final path never holds an unverified or partial file.
"""

from __future__ import annotations

import hashlib
import os
import platform
import re
import sys
import urllib.error
import urllib.parse
import urllib.request
import json
from pathlib import Path

# Configuration
REPO_OWNER = "quinnjr"
REPO_NAME = "dependency-injector"
PACKAGE_DIR = Path(__file__).parent.parent / "dependency_injector"

# The only host DI_GITHUB_TOKEN may ever be sent to.
GITHUB_API_HOST = "api.github.com"

# Maximum number of HTTP redirects to follow before giving up. Matches
# MAX_REDIRECTS in ffi/nodejs/scripts/install.js (urllib's default is 10).
MAX_REDIRECTS = 5


class DownloadIncompleteError(Exception):
    """The transfer ended before the advertised Content-Length was reached.

    This is a network failure (soft path, exit 0), explicitly distinct from a
    checksum failure so a truncated download is never reported as tampering.
    """


class _SafeRedirectHandler(urllib.request.HTTPRedirectHandler):
    """Redirect handler that never forwards credentials off api.github.com.

    CPython's ``HTTPRedirectHandler.redirect_request`` copies every request
    header except content-length/content-type onto the redirect target, with
    no cross-origin check, and permits an https -> http downgrade. Since the
    release-metadata request carries ``Authorization: token <DI_GITHUB_TOKEN>``,
    the default behaviour would hand that token to whatever host a redirect
    names. This subclass:

    - refuses to follow redirects whose target is not https, and
    - strips the ``Authorization`` header whenever the redirect target host is
      not api.github.com.
    """

    # Align urllib's redirect cap with the Node installer's MAX_REDIRECTS.
    max_redirections = MAX_REDIRECTS

    def redirect_request(self, req, fp, code, msg, headers, newurl):
        parts = urllib.parse.urlsplit(newurl)

        if parts.scheme != "https":
            raise urllib.error.HTTPError(
                newurl,
                code,
                f"refusing to follow non-https redirect to {newurl}",
                headers,
                fp,
            )

        new_req = super().redirect_request(req, fp, code, msg, headers, newurl)
        if new_req is None:
            return None

        if (parts.hostname or "").lower() != GITHUB_API_HOST:
            for store in (new_req.headers, new_req.unredirected_hdrs):
                for key in [k for k in store if k.lower() == "authorization"]:
                    del store[key]

        return new_req


# Every outbound request goes through this opener so the redirect cap and the
# credential-stripping rule above apply uniformly.
_OPENER = urllib.request.build_opener(_SafeRedirectHandler)


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


def staging_path(lib_path: Path) -> Path:
    """Return the staging path a download is written to before verification.

    Uses ``with_name`` rather than ``with_suffix`` so the library's full name is
    preserved (``with_suffix`` would *replace* a trailing version component such
    as ``libdependency_injector.so.2``), and appends the pid so two installers
    running concurrently in the same directory never share a staging file. This
    is the exact analogue of the Node installer's ``<output>.download.<pid>``.
    """
    return lib_path.with_name(f"{lib_path.name}.download.{os.getpid()}")


def get_release_assets(version: str) -> tuple[str, list[dict] | None]:
    """Fetch release metadata from GitHub.

    This is the only request that carries DI_GITHUB_TOKEN; `_OPENER` strips the
    header if the request is redirected off api.github.com.

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
        with _OPENER.open(req, timeout=30) as response:
            release = json.loads(response.read().decode())
        return tag, release.get("assets", [])
    except Exception as e:
        print(f"Warning: Could not fetch release info: {e}", file=sys.stderr)

    return tag, None


def fetch_text(url: str) -> str:
    """Download a URL body as text (used for the SHA256SUMS asset)."""
    headers = {"User-Agent": f"dependency-injector-python/{get_version()}"}
    req = urllib.request.Request(url, headers=headers)
    with _OPENER.open(req, timeout=30) as response:
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

    This helper is pure with respect to the filesystem: it never deletes the
    file it is checking. The caller owns cleanup (matching the Node installer,
    where verifyChecksum throws and the caller removes the staged file).

    - Release has no SHA256SUMS asset (pre-checksum release): warn loudly
      and return True (proceed), unless DI_REQUIRE_CHECKSUM is set
      (non-empty), in which case return False.
    - SHA256SUMS exists but cannot be fetched, has no entry for the asset,
      or the hash mismatches: return False.
    """
    sums_asset = next((a for a in assets if a["name"] == "SHA256SUMS"), None)

    if sums_asset is None:
        if os.environ.get("DI_REQUIRE_CHECKSUM"):
            print(f"❌ DI_REQUIRE_CHECKSUM is set but release {tag} has no SHA256SUMS asset.")
            return False
        print(
            f"⚠️  WARNING: release {tag} has no SHA256SUMS asset; "
            "skipping checksum verification (pre-checksum release). "
            "Set DI_REQUIRE_CHECKSUM=1 to make this a hard failure."
        )
        return True

    try:
        sums_text = fetch_text(sums_asset["browser_download_url"])
    except Exception as e:
        print(f"❌ Failed to download SHA256SUMS from release {tag}: {e}")
        return False

    expected = parse_sha256sums(sums_text, asset_name)
    if expected is None:
        print(f"❌ SHA256SUMS in release {tag} has no entry for '{asset_name}'.")
        return False

    actual = sha256_file(lib_path)
    if actual != expected:
        print(f"❌ Checksum mismatch for '{asset_name}' (release {tag}):")
        print(f"   expected {expected}")
        print(f"   actual   {actual}")
        print("   This may indicate a corrupted or tampered download.")
        return False

    print(f"✅ SHA256 checksum verified: {actual}")
    return True


def download_file(url: str, dest: Path) -> None:
    """Download a file from URL to destination.

    The destination is written as-is; callers are responsible for verifying
    it, cleaning it up on failure, and moving it into its final location.

    Raises:
        DownloadIncompleteError: the transfer ended before the advertised
            Content-Length was reached. This is a network failure, not a
            checksum failure, and is handled on the soft (exit 0) path.
    """
    print(f"Downloading from {url}...")

    headers = {"User-Agent": f"dependency-injector-python/{get_version()}"}
    req = urllib.request.Request(url, headers=headers)

    with _OPENER.open(req, timeout=60) as response:
        expected_bytes = _content_length(response)
        dest.parent.mkdir(parents=True, exist_ok=True)
        written = 0
        with open(dest, "wb") as f:
            while chunk := response.read(8192):
                f.write(chunk)
                written += len(chunk)

    if expected_bytes is not None and written < expected_bytes:
        raise DownloadIncompleteError(
            f"truncated download: received {written} of {expected_bytes} bytes from {url}"
        )


def _content_length(response) -> int | None:
    """Parse a response's Content-Length header, or None if absent/malformed."""
    raw = response.headers.get("Content-Length")
    if raw is None:
        return None
    try:
        return int(str(raw).strip())
    except (TypeError, ValueError):
        return None


def print_build_instructions() -> None:
    """Print the fallback options when no pre-built library is available."""
    print()
    print("You can:")
    print("  1. Build locally: cargo rustc --release --features ffi --crate-type cdylib")
    print("  2. Set DI_LIBRARY_PATH to point to an existing library")
    print("  3. Install a platform-specific wheel instead of sdist")
    print()
    print(
        "⚠️  NO NATIVE LIBRARY WAS INSTALLED. Exiting 0 so the install can "
        "continue, but importing dependency_injector will fail until a "
        "library is present."
    )


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
        # Missing release metadata or missing platform asset is a
        # network/availability problem, not a security one: warn and
        # continue (matches the Node installer).
        print(f"⚠️  Could not find asset '{asset_name}' in release {tag}")
        print_build_instructions()
        return 0

    # Download to a staging file next to the final path so the final path
    # never holds an unverified or partial file.
    tmp_path = staging_path(lib_path)

    try:
        download_file(download_url, tmp_path)
    except Exception as e:
        # Includes DownloadIncompleteError (truncated transfer): a network
        # failure, so the soft path applies.
        tmp_path.unlink(missing_ok=True)
        print(f"⚠️  Download failed: {e}")
        print_build_instructions()
        return 0

    try:
        # Verify FIRST, then set the file mode, then atomically move into
        # place. verify_checksum never deletes anything; cleanup is ours.
        if not verify_checksum(assets, tag, asset_name, tmp_path):
            tmp_path.unlink(missing_ok=True)
            print("   The downloaded file has been deleted.")
            return 1

        if sys.platform != "win32":
            tmp_path.chmod(0o755)
        tmp_path.replace(lib_path)
    except Exception as e:
        tmp_path.unlink(missing_ok=True)
        print(f"❌ Failed to install verified library: {e}")
        return 1

    print(f"✅ Downloaded to: {lib_path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
