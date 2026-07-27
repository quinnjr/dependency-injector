"""
Unit tests for scripts/download_native.py.

These tests are pure stdlib + pytest: no network access and no native
library are required. Network fetchers are monkeypatched where needed.

Run with: pytest tests/test_download_native.py
"""

from __future__ import annotations

import hashlib
import sys
from pathlib import Path

import pytest

# Add parent directory to path for imports (same convention as test_container.py)
sys.path.insert(0, str(Path(__file__).parent.parent))

from scripts import download_native as dn


KNOWN_CONTENT = b"abc"
KNOWN_DIGEST = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
ASSET = "libdependency_injector-linux-x64.so"
OTHER_DIGEST = "e" * 64


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


class TestVerifyChecksum:
    """Tests for verify_checksum (fetchers monkeypatched, no network)."""

    SUMS_ASSET = {
        "name": "SHA256SUMS",
        "browser_download_url": "https://example.invalid/SHA256SUMS",
    }

    @pytest.fixture
    def lib_file(self, tmp_path):
        path = tmp_path / "libdependency_injector.so.download"
        path.write_bytes(KNOWN_CONTENT)
        return path

    def test_match_returns_true_and_keeps_file(self, lib_file, monkeypatch):
        monkeypatch.setattr(dn, "fetch_text", lambda url: f"{KNOWN_DIGEST}  {ASSET}\n")
        assert dn.verify_checksum([self.SUMS_ASSET], "v1.0.0", ASSET, lib_file) is True
        assert lib_file.exists()

    def test_mismatch_returns_false_and_deletes_file(self, lib_file, monkeypatch):
        monkeypatch.setattr(dn, "fetch_text", lambda url: f"{OTHER_DIGEST}  {ASSET}\n")
        assert dn.verify_checksum([self.SUMS_ASSET], "v1.0.0", ASSET, lib_file) is False
        assert not lib_file.exists()

    def test_missing_entry_returns_false_and_deletes_file(self, lib_file, monkeypatch):
        monkeypatch.setattr(dn, "fetch_text", lambda url: f"{OTHER_DIGEST}  unrelated.dll\n")
        assert dn.verify_checksum([self.SUMS_ASSET], "v1.0.0", ASSET, lib_file) is False
        assert not lib_file.exists()

    def test_fetch_failure_returns_false_and_deletes_file(self, lib_file, monkeypatch):
        def boom(url):
            raise OSError("connection reset")

        monkeypatch.setattr(dn, "fetch_text", boom)
        assert dn.verify_checksum([self.SUMS_ASSET], "v1.0.0", ASSET, lib_file) is False
        assert not lib_file.exists()

    def test_no_sums_asset_warns_and_proceeds(self, lib_file, monkeypatch):
        monkeypatch.delenv("DI_REQUIRE_CHECKSUM", raising=False)
        monkeypatch.setattr(
            dn, "fetch_text", lambda url: pytest.fail("fetch_text should not be called")
        )
        assert dn.verify_checksum([], "v1.0.0", ASSET, lib_file) is True
        assert lib_file.exists()

    def test_no_sums_asset_with_require_checksum_hard_fails(self, lib_file, monkeypatch):
        monkeypatch.setenv("DI_REQUIRE_CHECKSUM", "1")
        monkeypatch.setattr(
            dn, "fetch_text", lambda url: pytest.fail("fetch_text should not be called")
        )
        assert dn.verify_checksum([], "v1.0.0", ASSET, lib_file) is False
        assert not lib_file.exists()

    def test_empty_require_checksum_is_not_strict(self, lib_file, monkeypatch):
        monkeypatch.setenv("DI_REQUIRE_CHECKSUM", "")
        assert dn.verify_checksum([], "v1.0.0", ASSET, lib_file) is True
        assert lib_file.exists()


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
