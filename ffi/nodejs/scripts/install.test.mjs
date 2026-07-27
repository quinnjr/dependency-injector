/**
 * Unit tests for scripts/install.js.
 *
 * Importing install.js does not run the installer: it only executes when
 * invoked directly as the main module (see the isMainModule guard).
 *
 * The checksum-enforcing paths are exercised against real files in a
 * throwaway temp directory, so deleting an enforcement branch fails a test.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { Readable } from "node:stream";
import {
  assetRequest,
  downloadFile,
  parseSha256Sums,
  sha256File,
  sha256Hex,
  stageAndInstall,
  stagingPathFor,
  verifyChecksum,
  VerificationError,
} from "./install.js";

// Known SHA-256 test vectors
const SHA256_ABC =
  "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
const SHA256_EMPTY =
  "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

const ASSET_NAME = "libdependency_injector-linux-x64.so";
const TAG = "v9.9.9";

/** A release asset list containing a SHA256SUMS entry. */
function assetsWithSums() {
  return [
    {
      name: ASSET_NAME,
      browser_download_url: `https://github.com/o/r/releases/download/${TAG}/${ASSET_NAME}`,
      url: "https://api.github.com/repos/o/r/releases/assets/1",
    },
    {
      name: "SHA256SUMS",
      browser_download_url: `https://github.com/o/r/releases/download/${TAG}/SHA256SUMS`,
      url: "https://api.github.com/repos/o/r/releases/assets/2",
    },
  ];
}

/** A fake http.IncomingMessage: a readable body plus headers. */
function fakeResponse(body, headers = {}) {
  const res = Readable.from([Buffer.from(body)], { objectMode: false });
  res.headers = headers;
  return res;
}

let tmpDir;

beforeEach(() => {
  tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "di-install-test-"));
});

afterEach(() => {
  vi.unstubAllEnvs();
  vi.restoreAllMocks();
  if (tmpDir) {
    fs.rmSync(tmpDir, { recursive: true, force: true });
    tmpDir = undefined;
  }
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

describe("sha256File", () => {
  it("hashes file contents and matches the in-memory digest", async () => {
    const file = path.join(tmpDir, "abc.bin");
    fs.writeFileSync(file, "abc");
    await expect(sha256File(file)).resolves.toBe(SHA256_ABC);
  });

  it("hashes an empty file", async () => {
    const file = path.join(tmpDir, "empty.bin");
    fs.writeFileSync(file, "");
    await expect(sha256File(file)).resolves.toBe(SHA256_EMPTY);
  });

  it("hashes a payload larger than one read chunk", async () => {
    const file = path.join(tmpDir, "big.bin");
    const payload = Buffer.alloc(256 * 1024, 0x5a);
    fs.writeFileSync(file, payload);
    await expect(sha256File(file)).resolves.toBe(sha256Hex(payload));
  });

  it("rejects when the file does not exist", async () => {
    await expect(sha256File(path.join(tmpDir, "missing.bin"))).rejects.toThrow();
  });
});

describe("assetRequest", () => {
  it("uses the public browser_download_url when no token is set", () => {
    vi.stubEnv("DI_GITHUB_TOKEN", "");
    const [asset] = assetsWithSums();
    const { url, options } = assetRequest(asset);
    expect(url).toBe(asset.browser_download_url);
    expect(options.headers?.Accept).toBeUndefined();
  });

  it("uses the api.github.com asset endpoint when a token is set", () => {
    vi.stubEnv("DI_GITHUB_TOKEN", "ghp_example");
    const [asset] = assetsWithSums();
    const { url, options } = assetRequest(asset);
    expect(url).toBe(asset.url);
    expect(new URL(url).hostname).toBe("api.github.com");
    expect(options.headers.Accept).toBe("application/octet-stream");
  });

  it("falls back to browser_download_url when the API url is absent", () => {
    vi.stubEnv("DI_GITHUB_TOKEN", "ghp_example");
    const asset = { name: ASSET_NAME, browser_download_url: "https://github.com/x" };
    expect(assetRequest(asset).url).toBe("https://github.com/x");
  });
});

describe("stagingPathFor", () => {
  it("stages next to the final path, qualified by pid", () => {
    const staged = stagingPathFor("/opt/native/lib.so");
    expect(staged).toBe(`/opt/native/lib.so.download.${process.pid}`);
    expect(path.dirname(staged)).toBe("/opt/native");
  });
});

describe("downloadFile", () => {
  it("writes the body and resolves when the length matches", async () => {
    const dest = path.join(tmpDir, "asset.so");
    const body = "complete payload";
    const getImpl = async () =>
      fakeResponse(body, { "content-length": String(Buffer.byteLength(body)) });

    await expect(downloadFile("https://example.invalid/a", dest, {}, getImpl))
      .resolves.toBeUndefined();
    expect(fs.readFileSync(dest, "utf8")).toBe(body);
  });

  it("resolves when the response has no content-length (chunked)", async () => {
    const dest = path.join(tmpDir, "asset.so");
    const getImpl = async () => fakeResponse("chunked payload", {});

    await expect(downloadFile("https://example.invalid/a", dest, {}, getImpl))
      .resolves.toBeUndefined();
    expect(fs.readFileSync(dest, "utf8")).toBe("chunked payload");
  });

  it("rejects a truncated body as a soft network error, not tampering", async () => {
    const dest = path.join(tmpDir, "asset.so");
    // Advertises 4096 bytes but the connection drops after a few.
    const getImpl = async () =>
      fakeResponse("short", { "content-length": "4096" });

    const err = await downloadFile("https://example.invalid/a", dest, {}, getImpl)
      .then(() => null, (e) => e);

    expect(err).toBeInstanceOf(Error);
    // Soft path (exit 0): a truncated download must NOT look like tampering.
    expect(err).not.toBeInstanceOf(VerificationError);
    expect(err.message).toContain("incomplete download");
    expect(err.message).toContain("4096");
    // The truncated file must not be left behind.
    expect(fs.existsSync(dest)).toBe(false);
  });

  it("passes request options through to the transport", async () => {
    const dest = path.join(tmpDir, "asset.so");
    const getImpl = vi.fn(async () => fakeResponse("x", {}));
    const options = { headers: { Accept: "application/octet-stream" } };

    await downloadFile("https://api.github.com/a", dest, options, getImpl);
    expect(getImpl).toHaveBeenCalledWith("https://api.github.com/a", options);
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

describe("verifyChecksum (SHA256SUMS present)", () => {
  /** Write `contents` to a staged file and return its path. */
  function stagedFile(contents) {
    const file = path.join(tmpDir, `${ASSET_NAME}.download`);
    fs.writeFileSync(file, contents);
    return file;
  }

  it("resolves and leaves the file in place when the digest matches", async () => {
    const contents = "genuine library bytes";
    const file = stagedFile(contents);
    const sums = `${sha256Hex(contents)}  ${ASSET_NAME}\n`;
    const log = vi.spyOn(console, "log").mockImplementation(() => {});

    await expect(
      verifyChecksum(assetsWithSums(), TAG, ASSET_NAME, file, async () => sums)
    ).resolves.toBeUndefined();

    expect(fs.existsSync(file)).toBe(true);
    expect(fs.readFileSync(file, "utf8")).toBe(contents);
    expect(log.mock.calls.some((c) => String(c[0]).includes("checksum verified"))).toBe(true);
  });

  it("rejects with VerificationError when the digest mismatches", async () => {
    const file = stagedFile("tampered library bytes");
    const sums = `${sha256Hex("genuine library bytes")}  ${ASSET_NAME}\n`;

    const err = await verifyChecksum(
      assetsWithSums(),
      TAG,
      ASSET_NAME,
      file,
      async () => sums
    ).then(() => null, (e) => e);

    expect(err).toBeInstanceOf(VerificationError);
    expect(err.message).toContain("checksum mismatch");
    expect(err.message).toContain(ASSET_NAME);
    expect(err.message).toContain(sha256Hex("genuine library bytes"));
  });

  it("rejects when SHA256SUMS has no entry for the asset", async () => {
    const file = stagedFile("genuine library bytes");
    const sums = `${SHA256_ABC}  some-other-asset.so\n`;

    const err = await verifyChecksum(
      assetsWithSums(),
      TAG,
      ASSET_NAME,
      file,
      async () => sums
    ).then(() => null, (e) => e);

    expect(err).toBeInstanceOf(VerificationError);
    expect(err.message).toContain("no entry for");
    expect(err.message).toContain(ASSET_NAME);
  });

  it("rejects as VerificationError (not the raw error) when the sums fetch fails", async () => {
    const file = stagedFile("genuine library bytes");

    const err = await verifyChecksum(assetsWithSums(), TAG, ASSET_NAME, file, async () => {
      throw new Error("socket hang up");
    }).then(() => null, (e) => e);

    // The wrapping is what routes this to exit 1 instead of exit 0.
    expect(err).toBeInstanceOf(VerificationError);
    expect(err.name).toBe("VerificationError");
    expect(err.message).toContain("failed to download SHA256SUMS");
    expect(err.message).toContain("socket hang up");
  });

  it("fetches SHA256SUMS through the same URL policy as the asset", async () => {
    const file = stagedFile("genuine library bytes");
    const sums = `${sha256Hex("genuine library bytes")}  ${ASSET_NAME}\n`;
    const fetchImpl = vi.fn(async () => sums);
    vi.spyOn(console, "log").mockImplementation(() => {});
    vi.stubEnv("DI_GITHUB_TOKEN", "ghp_example");

    await verifyChecksum(assetsWithSums(), TAG, ASSET_NAME, file, fetchImpl);

    const [url, options] = fetchImpl.mock.calls[0];
    expect(new URL(url).hostname).toBe("api.github.com");
    expect(options.headers.Accept).toBe("application/octet-stream");
  });
});

describe("stageAndInstall", () => {
  const CONTENTS = "genuine library bytes";

  /** downloadImpl stand-in that writes `contents` to the staging path. */
  function writingDownload(contents) {
    return vi.fn(async (_url, dest) => {
      fs.writeFileSync(dest, contents);
    });
  }

  function paths() {
    const outputPath = path.join(tmpDir, "libdependency_injector.so");
    return { outputPath, tempPath: stagingPathFor(outputPath) };
  }

  it("verifies, chmods and renames the staged file into place", async () => {
    const { outputPath, tempPath } = paths();
    vi.spyOn(console, "log").mockImplementation(() => {});

    await expect(
      stageAndInstall({
        asset: assetsWithSums()[0],
        assets: assetsWithSums(),
        tag: TAG,
        assetName: ASSET_NAME,
        outputPath,
        tempPath,
        downloadImpl: writingDownload(CONTENTS),
        fetchImpl: async () => `${sha256Hex(CONTENTS)}  ${ASSET_NAME}\n`,
      })
    ).resolves.toBe(outputPath);

    expect(fs.readFileSync(outputPath, "utf8")).toBe(CONTENTS);
    expect(fs.existsSync(tempPath)).toBe(false);
    if (process.platform !== "win32") {
      expect(fs.statSync(outputPath).mode & 0o777).toBe(0o755);
    }
  });

  it("deletes the staged file and never publishes it on checksum mismatch", async () => {
    const { outputPath, tempPath } = paths();

    const err = await stageAndInstall({
      asset: assetsWithSums()[0],
      assets: assetsWithSums(),
      tag: TAG,
      assetName: ASSET_NAME,
      outputPath,
      tempPath,
      downloadImpl: writingDownload("tampered library bytes"),
      fetchImpl: async () => `${sha256Hex(CONTENTS)}  ${ASSET_NAME}\n`,
    }).then(() => null, (e) => e);

    expect(err).toBeInstanceOf(VerificationError);
    expect(err.message).toContain("checksum mismatch");
    // The staged temp file must be gone...
    expect(fs.existsSync(tempPath)).toBe(false);
    // ...and the unverified bytes must never reach the final path.
    expect(fs.existsSync(outputPath)).toBe(false);
  });

  it("cleans up and re-throws the raw error when the sums fetch fails", async () => {
    const { outputPath, tempPath } = paths();

    const err = await stageAndInstall({
      asset: assetsWithSums()[0],
      assets: assetsWithSums(),
      tag: TAG,
      assetName: ASSET_NAME,
      outputPath,
      tempPath,
      downloadImpl: writingDownload(CONTENTS),
      fetchImpl: async () => {
        throw new Error("socket hang up");
      },
    }).then(() => null, (e) => e);

    expect(err).toBeInstanceOf(VerificationError);
    expect(fs.existsSync(tempPath)).toBe(false);
    expect(fs.existsSync(outputPath)).toBe(false);
  });

  it("propagates a download failure as a soft error and cleans up", async () => {
    const { outputPath, tempPath } = paths();

    const err = await stageAndInstall({
      asset: assetsWithSums()[0],
      assets: assetsWithSums(),
      tag: TAG,
      assetName: ASSET_NAME,
      outputPath,
      tempPath,
      downloadImpl: async (_url, dest) => {
        fs.writeFileSync(dest, "partial");
        throw new Error("incomplete download of asset: expected 4096 bytes, received 7");
      },
      fetchImpl: async () => `${sha256Hex(CONTENTS)}  ${ASSET_NAME}\n`,
    }).then(() => null, (e) => e);

    expect(err).toBeInstanceOf(Error);
    expect(err).not.toBeInstanceOf(VerificationError);
    expect(fs.existsSync(tempPath)).toBe(false);
    expect(fs.existsSync(outputPath)).toBe(false);
  });

  it("downloads from the URL chosen by the token policy", async () => {
    const { outputPath, tempPath } = paths();
    vi.spyOn(console, "log").mockImplementation(() => {});
    vi.stubEnv("DI_GITHUB_TOKEN", "ghp_example");
    const downloadImpl = writingDownload(CONTENTS);

    await stageAndInstall({
      asset: assetsWithSums()[0],
      assets: assetsWithSums(),
      tag: TAG,
      assetName: ASSET_NAME,
      outputPath,
      tempPath,
      downloadImpl,
      fetchImpl: async () => `${sha256Hex(CONTENTS)}  ${ASSET_NAME}\n`,
    });

    const [url, , options] = downloadImpl.mock.calls[0];
    expect(url).toBe(assetsWithSums()[0].url);
    expect(options.headers.Accept).toBe("application/octet-stream");
  });

  it("stages under a pid-qualified sibling path by default", async () => {
    const outputPath = path.join(tmpDir, "libdependency_injector.so");
    vi.spyOn(console, "log").mockImplementation(() => {});
    const downloadImpl = writingDownload(CONTENTS);

    await stageAndInstall({
      asset: assetsWithSums()[0],
      assets: assetsWithSums(),
      tag: TAG,
      assetName: ASSET_NAME,
      outputPath,
      downloadImpl,
      fetchImpl: async () => `${sha256Hex(CONTENTS)}  ${ASSET_NAME}\n`,
    });

    const [, dest] = downloadImpl.mock.calls[0];
    expect(dest).toBe(`${outputPath}.download.${process.pid}`);
    expect(fs.existsSync(outputPath)).toBe(true);
  });
});
