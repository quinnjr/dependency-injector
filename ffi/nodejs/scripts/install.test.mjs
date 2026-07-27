/**
 * Unit tests for the pure helpers in scripts/install.js.
 *
 * Importing install.js does not run the installer: it only executes when
 * invoked directly as the main module (see the import.meta.url guard).
 */

import { describe, it, expect, vi, afterEach } from "vitest";
import {
  parseSha256Sums,
  sha256Hex,
  verifyChecksum,
  VerificationError,
} from "./install.js";

// Known SHA-256 test vectors
const SHA256_ABC =
  "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
const SHA256_EMPTY =
  "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

afterEach(() => {
  vi.unstubAllEnvs();
  vi.restoreAllMocks();
});

describe("parseSha256Sums", () => {
  it("parses the standard two-space sha256sum format", () => {
    const text = `${SHA256_ABC}  libdependency_injector-linux-x64.so\n`;
    expect(parseSha256Sums(text, "libdependency_injector-linux-x64.so")).toBe(
      SHA256_ABC
    );
  });

  it("parses the binary-mode '*' marker format", () => {
    const text = `${SHA256_ABC} *libdependency_injector-linux-x64.so\n`;
    expect(parseSha256Sums(text, "libdependency_injector-linux-x64.so")).toBe(
      SHA256_ABC
    );
  });

  it("returns null when the asset has no entry", () => {
    const text = `${SHA256_ABC}  some-other-file.so\n`;
    expect(parseSha256Sums(text, "libdependency_injector-linux-x64.so")).toBeNull();
  });

  it("returns null for empty input", () => {
    expect(parseSha256Sums("", "anything.so")).toBeNull();
  });

  it("selects the right entry from a multi-line file", () => {
    const text = [
      `${SHA256_EMPTY}  libdependency_injector-darwin-arm64.dylib`,
      `${SHA256_ABC}  libdependency_injector-linux-x64.so`,
      `${SHA256_EMPTY} *dependency_injector-win32-x64.dll`,
    ].join("\n");
    expect(parseSha256Sums(text, "libdependency_injector-linux-x64.so")).toBe(
      SHA256_ABC
    );
    expect(parseSha256Sums(text, "dependency_injector-win32-x64.dll")).toBe(
      SHA256_EMPTY
    );
  });

  it("lowercases uppercase hex digests", () => {
    const text = `${SHA256_ABC.toUpperCase()}  file.so\n`;
    expect(parseSha256Sums(text, "file.so")).toBe(SHA256_ABC);
  });

  it("ignores malformed lines (wrong hash length, comments, blanks)", () => {
    const text = [
      "# comment line",
      "",
      "deadbeef  file.so", // hash too short
      `${SHA256_ABC}file.so`, // no separator
    ].join("\n");
    expect(parseSha256Sums(text, "file.so")).toBeNull();
  });
});

describe("sha256Hex", () => {
  it("computes the known digest of 'abc'", () => {
    expect(sha256Hex("abc")).toBe(SHA256_ABC);
  });

  it("computes the known digest of the empty string", () => {
    expect(sha256Hex("")).toBe(SHA256_EMPTY);
  });

  it("accepts Buffers and matches the string digest", () => {
    expect(sha256Hex(Buffer.from("abc", "utf8"))).toBe(SHA256_ABC);
  });
});

describe("checksum comparison", () => {
  it("detects a mismatch between downloaded content and the sums entry", () => {
    const sums = `${sha256Hex("expected contents")}  asset.so\n`;
    const expected = parseSha256Sums(sums, "asset.so");
    const actual = sha256Hex("tampered contents");
    expect(actual).not.toBe(expected);
  });

  it("matches when the content is what the sums entry describes", () => {
    const sums = `${sha256Hex("expected contents")}  asset.so\n`;
    const expected = parseSha256Sums(sums, "asset.so");
    const actual = sha256Hex("expected contents");
    expect(actual).toBe(expected);
  });
});

describe("verifyChecksum (no SHA256SUMS asset)", () => {
  it("warns and proceeds when DI_REQUIRE_CHECKSUM is unset", async () => {
    vi.stubEnv("DI_REQUIRE_CHECKSUM", "");
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});

    await expect(
      verifyChecksum([], "v0.0.0", "asset.so", "/nonexistent/asset.so")
    ).resolves.toBeUndefined();
    expect(warn).toHaveBeenCalledOnce();
    expect(warn.mock.calls[0][0]).toContain("no SHA256SUMS asset");
  });

  it("hard-fails when DI_REQUIRE_CHECKSUM is set", async () => {
    vi.stubEnv("DI_REQUIRE_CHECKSUM", "1");

    await expect(
      verifyChecksum([], "v0.0.0", "asset.so", "/nonexistent/asset.so")
    ).rejects.toBeInstanceOf(VerificationError);
  });
});
