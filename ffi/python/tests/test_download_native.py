"""
Unit tests for scripts/download_native.py.

These tests are pure stdlib + pytest: no network access and no native
library are required. Network fetchers are monkeypatched where needed.

Run with: pytest tests/test_download_native.py
"""

from __future__ import annotations

import email.message
import hashlib
import io
import os
import sys
import urllib.error
import urllib.request
from pathlib import Path

import pytest

# Add parent directory to path for imports (same convention as test_container.py)
sys.path.insert(0, str(Path(__file__).parent.parent))

from scripts import download_native as dn


KNOWN_CONTENT = b"abc"
KNOWN_DIGEST = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
ASSET = "libdependency_injector-linux-x64.so"
OTHER_DIGEST = "e" * 64
LIB_NAME = "libdependency_injector.so"
API_URL = f"https://api.github.com/repos/{dn.REPO_OWNER}/{dn.REPO_NAME}/releases/tags/v1.0.0"


def _authorization_values(request: urllib.request.Request) -> list[str]:
    """All Authorization header values on a Request, case-insensitively."""
    values = []
    for store in (request.headers, request.unredirected_hdrs):
        values.extend(v for k, v in store.items() if k.lower() == "authorization")
    return values


def _api_request(with_token: bool = True) -> urllib.request.Request:
    headers = {
        "Accept": "application/vnd.github.v3+json",
        "User-Agent": "dependency-injector-python/1.0.0",
    }
    if with_token:
        headers["Authorization"] = "token SUPER-SECRET"
    return urllib.request.Request(API_URL, headers=headers)


class _FakeResponse:
    """Minimal stand-in for an http.client.HTTPResponse."""

    def __init__(self, body: bytes, content_length: object | None = None):
        self._buf = io.BytesIO(body)
        self.headers = email.message.Message()
        if content_length is not None:
            self.headers["Content-Length"] = str(content_length)

    def read(self, size: int = -1) -> bytes:
        return self._buf.read(size)

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        return False


class _FakeOpener:
    """Stand-in for dn._OPENER that returns a canned response."""

    def __init__(self, response_factory):
        self._response_factory = response_factory
        self.requests: list[urllib.request.Request] = []

    def open(self, req, timeout=None):
        self.requests.append(req)
        return self._response_factory()


class TestParseSha256Sums:
    """Tests for parse_sha256sums."""

    def test_two_space_format(self):
        text = f"{KNOWN_DIGEST}  {ASSET}\n{OTHER_DIGEST}  other-file.dll\n"
        assert dn.parse_sha256sums(text, ASSET) == KNOWN_DIGEST

    def test_binary_marker(self):
        text = f"{KNOWN_DIGEST} *{ASSET}\n"
        assert dn.parse_sha256sums(text, ASSET) == KNOWN_DIGEST

    def test_missing_name_returns_none(self):
        text = f"{KNOWN_DIGEST}  some-other-asset.so\n"
        assert dn.parse_sha256sums(text, ASSET) is None

    def test_empty_text_returns_none(self):
        assert dn.parse_sha256sums("", ASSET) is None

    def test_uppercase_hex_normalized(self):
        text = f"{KNOWN_DIGEST.upper()}  {ASSET}\n"
        assert dn.parse_sha256sums(text, ASSET) == KNOWN_DIGEST

    def test_malformed_lines_skipped(self):
        text = "\n".join(
            [
                "not a checksum line",
                f"deadbeef  {ASSET}",  # hex too short
                f"{KNOWN_DIGEST}  {ASSET}",
            ]
        )
        assert dn.parse_sha256sums(text, ASSET) == KNOWN_DIGEST


class TestSha256File:
    """Tests for sha256_file."""

    def test_known_digest(self, tmp_path):
        target = tmp_path / "blob.bin"
        target.write_bytes(KNOWN_CONTENT)
        assert dn.sha256_file(target) == KNOWN_DIGEST

    def test_matches_hashlib_for_larger_file(self, tmp_path):
        data = b"\x00\x01\x02" * 100_000  # spans multiple 64 KiB read chunks
        target = tmp_path / "big.bin"
        target.write_bytes(data)
        assert dn.sha256_file(target) == hashlib.sha256(data).hexdigest()


class TestSafeRedirectHandler:
    """The redirect handler must never forward DI_GITHUB_TOKEN off-host."""

    HEADERS = email.message.Message()

    def _redirect(self, newurl, *, with_token=True, code=302):
        handler = dn._SafeRedirectHandler()
        return handler.redirect_request(
            _api_request(with_token=with_token), None, code, "Found", self.HEADERS, newurl
        )

    def test_cross_host_redirect_strips_authorization(self):
        new_req = self._redirect("https://evil.invalid/assets/lib.so")
        assert new_req is not None
        assert new_req.full_url == "https://evil.invalid/assets/lib.so"
        assert _authorization_values(new_req) == []
        assert new_req.get_header("Authorization") is None

    def test_cross_host_redirect_keeps_other_headers(self):
        new_req = self._redirect("https://objects.githubusercontent.com/x")
        assert new_req.get_header("User-agent") == "dependency-injector-python/1.0.0"
        assert _authorization_values(new_req) == []

    def test_same_host_redirect_preserves_authorization(self):
        new_req = self._redirect("https://api.github.com/repositories/1/releases/2")
        assert new_req is not None
        assert _authorization_values(new_req) == ["token SUPER-SECRET"]

    def test_same_host_uppercase_preserves_authorization(self):
        new_req = self._redirect("https://API.GitHub.com/repositories/1/releases/2")
        assert _authorization_values(new_req) == ["token SUPER-SECRET"]

    def test_subdomain_of_api_host_is_cross_host(self):
        new_req = self._redirect("https://api.github.com.evil.invalid/x")
        assert _authorization_values(new_req) == []

    def test_http_downgrade_is_refused(self):
        with pytest.raises(urllib.error.HTTPError):
            self._redirect("http://api.github.com/repositories/1")

    def test_non_http_scheme_is_refused(self):
        with pytest.raises(urllib.error.HTTPError):
            self._redirect("ftp://api.github.com/repositories/1")

    def test_untokened_request_survives_redirect(self):
        new_req = self._redirect("https://elsewhere.invalid/x", with_token=False)
        assert new_req is not None
        assert _authorization_values(new_req) == []

    def test_redirect_cap_matches_node_installer(self):
        assert dn.MAX_REDIRECTS == 5
        assert dn._SafeRedirectHandler.max_redirections == dn.MAX_REDIRECTS

    def test_opener_installs_the_safe_handler(self):
        assert any(isinstance(h, dn._SafeRedirectHandler) for h in dn._OPENER.handlers)


class TestStagingPath:
    """Staging names must append (never replace) and be pid-unique."""

    def test_appends_rather_than_replaces_suffix(self):
        staged = dn.staging_path(Path("/n") / LIB_NAME)
        assert staged.name == f"{LIB_NAME}.download.{os.getpid()}"
        assert staged.parent == Path("/n")

    def test_versioned_library_name_is_preserved(self):
        versioned = Path("/n/libdependency_injector.so.2.0")
        staged = dn.staging_path(versioned)
        assert staged.name.startswith("libdependency_injector.so.2.0.download.")
        # with_suffix() would have produced "libdependency_injector.so.2.download"
        assert "libdependency_injector.so.2.0" in staged.name

    def test_pid_suffix_present(self):
        staged = dn.staging_path(Path("/n") / LIB_NAME)
        assert staged.name.endswith(f".download.{os.getpid()}")


class TestDownloadFile:
    """Truncated transfers must be reported as network errors, not tampering."""

    def _patch_opener(self, monkeypatch, response_factory):
        opener = _FakeOpener(response_factory)
        monkeypatch.setattr(dn, "_OPENER", opener)
        monkeypatch.setattr(dn, "get_version", lambda: "1.0.0")
        return opener

    def test_complete_download_writes_file(self, tmp_path, monkeypatch):
        self._patch_opener(
            monkeypatch, lambda: _FakeResponse(KNOWN_CONTENT, content_length=len(KNOWN_CONTENT))
        )
        dest = tmp_path / "native" / LIB_NAME
        dn.download_file("https://example.invalid/lib.so", dest)
        assert dest.read_bytes() == KNOWN_CONTENT

    def test_short_read_raises_download_incomplete(self, tmp_path, monkeypatch):
        self._patch_opener(monkeypatch, lambda: _FakeResponse(b"ab", content_length=1024))
        dest = tmp_path / LIB_NAME
        with pytest.raises(dn.DownloadIncompleteError) as excinfo:
            dn.download_file("https://example.invalid/lib.so", dest)
        message = str(excinfo.value)
        assert "truncated" in message
        assert "2 of 1024 bytes" in message
        # Distinct from checksum failures: no tampering language.
        assert "checksum" not in message.lower()
        assert "tamper" not in message.lower()

    def test_missing_content_length_accepts_body(self, tmp_path, monkeypatch):
        self._patch_opener(monkeypatch, lambda: _FakeResponse(KNOWN_CONTENT))
        dest = tmp_path / LIB_NAME
        dn.download_file("https://example.invalid/lib.so", dest)
        assert dest.read_bytes() == KNOWN_CONTENT

    def test_malformed_content_length_ignored(self, tmp_path, monkeypatch):
        self._patch_opener(
            monkeypatch, lambda: _FakeResponse(KNOWN_CONTENT, content_length="not-a-number")
        )
        dest = tmp_path / LIB_NAME
        dn.download_file("https://example.invalid/lib.so", dest)
        assert dest.read_bytes() == KNOWN_CONTENT

    def test_longer_than_advertised_is_not_an_error(self, tmp_path, monkeypatch):
        self._patch_opener(monkeypatch, lambda: _FakeResponse(KNOWN_CONTENT, content_length=1))
        dest = tmp_path / LIB_NAME
        dn.download_file("https://example.invalid/lib.so", dest)
        assert dest.read_bytes() == KNOWN_CONTENT

    def test_download_does_not_send_authorization(self, tmp_path, monkeypatch):
        monkeypatch.setenv("DI_GITHUB_TOKEN", "SUPER-SECRET")
        opener = self._patch_opener(
            monkeypatch, lambda: _FakeResponse(KNOWN_CONTENT, content_length=len(KNOWN_CONTENT))
        )
        dn.download_file("https://example.invalid/lib.so", tmp_path / LIB_NAME)
        assert _authorization_values(opener.requests[0]) == []


class TestVerifyChecksum:
    """verify_checksum is pure: it reports, it never deletes (caller cleans up)."""

    SUMS_ASSET = {
        "name": "SHA256SUMS",
        "browser_download_url": "https://example.invalid/SHA256SUMS",
    }

    @pytest.fixture
    def lib_file(self, tmp_path):
        path = tmp_path / f"{LIB_NAME}.download.{os.getpid()}"
        path.write_bytes(KNOWN_CONTENT)
        return path

    def test_match_returns_true_and_keeps_file(self, lib_file, monkeypatch):
        monkeypatch.setattr(dn, "fetch_text", lambda url: f"{KNOWN_DIGEST}  {ASSET}\n")
        assert dn.verify_checksum([self.SUMS_ASSET], "v1.0.0", ASSET, lib_file) is True
        assert lib_file.exists()

    def test_mismatch_returns_false_without_deleting(self, lib_file, monkeypatch):
        monkeypatch.setattr(dn, "fetch_text", lambda url: f"{OTHER_DIGEST}  {ASSET}\n")
        assert dn.verify_checksum([self.SUMS_ASSET], "v1.0.0", ASSET, lib_file) is False
        # Purity: deletion is the caller's job (see TestMainInstallFlow).
        assert lib_file.exists()

    def test_missing_entry_returns_false_without_deleting(self, lib_file, monkeypatch):
        monkeypatch.setattr(dn, "fetch_text", lambda url: f"{OTHER_DIGEST}  unrelated.dll\n")
        assert dn.verify_checksum([self.SUMS_ASSET], "v1.0.0", ASSET, lib_file) is False
        assert lib_file.exists()

    def test_fetch_failure_returns_false_without_deleting(self, lib_file, monkeypatch):
        def boom(url):
            raise OSError("connection reset")

        monkeypatch.setattr(dn, "fetch_text", boom)
        assert dn.verify_checksum([self.SUMS_ASSET], "v1.0.0", ASSET, lib_file) is False
        assert lib_file.exists()

    def test_no_sums_asset_warns_and_proceeds(self, lib_file, monkeypatch, capsys):
        monkeypatch.delenv("DI_REQUIRE_CHECKSUM", raising=False)
        monkeypatch.setattr(
            dn, "fetch_text", lambda url: pytest.fail("fetch_text should not be called")
        )
        assert dn.verify_checksum([], "v1.0.0", ASSET, lib_file) is True
        assert lib_file.exists()
        assert "Set DI_REQUIRE_CHECKSUM=1 to make this a hard failure." in capsys.readouterr().out

    def test_no_sums_asset_with_require_checksum_hard_fails(self, lib_file, monkeypatch):
        monkeypatch.setenv("DI_REQUIRE_CHECKSUM", "1")
        monkeypatch.setattr(
            dn, "fetch_text", lambda url: pytest.fail("fetch_text should not be called")
        )
        assert dn.verify_checksum([], "v1.0.0", ASSET, lib_file) is False
        assert lib_file.exists()

    def test_empty_require_checksum_is_not_strict(self, lib_file, monkeypatch):
        monkeypatch.setenv("DI_REQUIRE_CHECKSUM", "")
        assert dn.verify_checksum([], "v1.0.0", ASSET, lib_file) is True
        assert lib_file.exists()


class TestMainInstallFlow:
    """End-to-end main() behaviour with all network calls monkeypatched."""

    SUMS_ASSET = {
        "name": "SHA256SUMS",
        "browser_download_url": "https://example.invalid/SHA256SUMS",
    }
    LIB_ASSET = {"name": ASSET, "browser_download_url": "https://example.invalid/lib.so"}

    @pytest.fixture
    def package_dir(self, tmp_path, monkeypatch):
        # Nest deeply so main()'s 3..5-level parent walk for target/release
        # stays inside tmp_path and can never find a real local build.
        pkg = tmp_path / "a" / "b" / "c" / "d" / "e" / "dependency_injector"
        pkg.mkdir(parents=True)
        monkeypatch.setattr(dn, "PACKAGE_DIR", pkg)
        monkeypatch.setattr(dn, "get_version", lambda: "1.0.0")
        monkeypatch.setattr(dn, "get_platform_info", lambda: ("linux-x64", ASSET, LIB_NAME))
        monkeypatch.delenv("DI_SKIP_DOWNLOAD", raising=False)
        monkeypatch.delenv("DI_LIBRARY_PATH", raising=False)
        monkeypatch.delenv("DI_REQUIRE_CHECKSUM", raising=False)
        return pkg

    def _patch_network(self, monkeypatch, *, assets, sums_text=None, download=None):
        monkeypatch.setattr(dn, "get_release_assets", lambda version: ("v1.0.0", assets))

        def default_download(url, dest):
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_bytes(KNOWN_CONTENT)

        monkeypatch.setattr(dn, "download_file", download or default_download)
        if sums_text is not None:
            monkeypatch.setattr(dn, "fetch_text", lambda url: sums_text)

    @staticmethod
    def _staging_leftovers(native_dir: Path) -> list[Path]:
        if not native_dir.exists():
            return []
        return [p for p in native_dir.iterdir() if ".download" in p.name]

    def test_success_installs_verified_library(self, package_dir, monkeypatch):
        self._patch_network(
            monkeypatch,
            assets=[self.LIB_ASSET, self.SUMS_ASSET],
            sums_text=f"{KNOWN_DIGEST}  {ASSET}\n",
        )
        assert dn.main() == 0
        lib_path = package_dir / "native" / LIB_NAME
        assert lib_path.read_bytes() == KNOWN_CONTENT
        assert self._staging_leftovers(lib_path.parent) == []

    def test_checksum_mismatch_exits_1_and_leaves_no_file(self, package_dir, monkeypatch, capsys):
        self._patch_network(
            monkeypatch,
            assets=[self.LIB_ASSET, self.SUMS_ASSET],
            sums_text=f"{OTHER_DIGEST}  {ASSET}\n",
        )
        assert dn.main() == 1
        native_dir = package_dir / "native"
        # Caller owns cleanup: nothing at the final path, no staged file left.
        assert not (native_dir / LIB_NAME).exists()
        assert self._staging_leftovers(native_dir) == []
        out = capsys.readouterr().out
        assert "Checksum mismatch" in out
        assert "The downloaded file has been deleted." in out

    def test_missing_sums_entry_exits_1_and_leaves_no_file(self, package_dir, monkeypatch):
        self._patch_network(
            monkeypatch,
            assets=[self.LIB_ASSET, self.SUMS_ASSET],
            sums_text=f"{OTHER_DIGEST}  unrelated.dll\n",
        )
        assert dn.main() == 1
        native_dir = package_dir / "native"
        assert not (native_dir / LIB_NAME).exists()
        assert self._staging_leftovers(native_dir) == []

    def test_require_checksum_without_sums_exits_1_and_leaves_no_file(
        self, package_dir, monkeypatch
    ):
        monkeypatch.setenv("DI_REQUIRE_CHECKSUM", "1")
        self._patch_network(monkeypatch, assets=[self.LIB_ASSET])
        assert dn.main() == 1
        native_dir = package_dir / "native"
        assert not (native_dir / LIB_NAME).exists()
        assert self._staging_leftovers(native_dir) == []

    def test_truncated_download_is_soft_failure(self, package_dir, monkeypatch, capsys):
        def truncated(url, dest):
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_bytes(b"ab")
            raise dn.DownloadIncompleteError(
                "truncated download: received 2 of 1024 bytes from https://example.invalid/lib.so"
            )

        self._patch_network(
            monkeypatch, assets=[self.LIB_ASSET, self.SUMS_ASSET], download=truncated
        )
        assert dn.main() == 0
        native_dir = package_dir / "native"
        assert not (native_dir / LIB_NAME).exists()
        assert self._staging_leftovers(native_dir) == []
        out = capsys.readouterr().out
        assert "Download failed" in out
        assert "truncated download" in out
        assert "tamper" not in out.lower()
        assert "NO NATIVE LIBRARY WAS INSTALLED" in out

    def test_missing_asset_is_soft_failure_with_clear_message(
        self, package_dir, monkeypatch, capsys
    ):
        self._patch_network(monkeypatch, assets=[])
        assert dn.main() == 0
        out = capsys.readouterr().out
        assert "NO NATIVE LIBRARY WAS INSTALLED" in out
        assert not (package_dir / "native" / LIB_NAME).exists()

    def test_unfetchable_release_metadata_is_soft_failure(self, package_dir, monkeypatch, capsys):
        self._patch_network(monkeypatch, assets=None)
        assert dn.main() == 0
        assert "NO NATIVE LIBRARY WAS INSTALLED" in capsys.readouterr().out

    def test_no_sums_asset_installs_with_warning(self, package_dir, monkeypatch, capsys):
        self._patch_network(monkeypatch, assets=[self.LIB_ASSET])
        assert dn.main() == 0
        lib_path = package_dir / "native" / LIB_NAME
        assert lib_path.read_bytes() == KNOWN_CONTENT
        assert "Set DI_REQUIRE_CHECKSUM=1 to make this a hard failure." in capsys.readouterr().out

    def test_unsupported_platform_exits_1(self, package_dir, monkeypatch):
        def unsupported():
            raise RuntimeError("Unsupported platform: solaris")

        monkeypatch.setattr(dn, "get_platform_info", unsupported)
        assert dn.main() == 1


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
