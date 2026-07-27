#!/usr/bin/env node

/**
 * Post-install script to download pre-built native library.
 *
 * This script automatically downloads the correct pre-built native library
 * for the current platform and architecture from GitHub releases.
 *
 * Environment variables:
 * - DI_LIBRARY_PATH: Skip download and use this path instead
 * - DI_SKIP_DOWNLOAD: Skip download (for local development)
 * - DI_GITHUB_TOKEN: GitHub token for private repos or rate limiting.
 *   Only ever sent to api.github.com — never to download hosts or
 *   arbitrary redirect targets. When set, assets are fetched through the
 *   api.github.com asset endpoint (which the token authenticates) rather
 *   than the public browser_download_url, so private repos work.
 * - DI_REQUIRE_CHECKSUM: When set (non-empty), a release that has no
 *   SHA256SUMS asset is a hard failure (exit 1) instead of a warning.
 *
 * Failure policy:
 * - Network/download failure (release metadata fetch fails, asset missing
 *   from the release, truncated body, download or filesystem error): warn
 *   and exit 0 — the library can be built from source or fetched later.
 * - Checksum mismatch on a *complete* download, SHA256SUMS present but
 *   missing an entry for the asset, or failure to fetch an existing
 *   SHA256SUMS asset: hard failure (exit 1) and the downloaded file is
 *   deleted.
 * - Release has no SHA256SUMS asset at all (pre-checksum release): warn
 *   and proceed, unless DI_REQUIRE_CHECKSUM is set (then exit 1).
 *
 * Downloads are staged at "<output>.download.<pid>" and only renamed into
 * place after checksum verification succeeds, so the final library path
 * never holds an unverified file. On any failure the staged file is deleted.
 */

import https from 'https';
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import { createReadStream, createWriteStream } from 'fs';
import { createHash } from 'node:crypto';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Configuration
const REPO_OWNER = 'pegasusheavy';
const REPO_NAME = 'dependency-injector';
const PACKAGE_VERSION = JSON.parse(
  fs.readFileSync(path.join(__dirname, '..', 'package.json'), 'utf8')
).version;

// Maximum number of HTTP redirects to follow before giving up.
const MAX_REDIRECTS = 5;

// Platform/arch mapping to asset names
const PLATFORM_MAP = {
  'linux-x64': 'libdependency_injector-linux-x64.so',
  'linux-arm64': 'libdependency_injector-linux-arm64.so',
  'darwin-x64': 'libdependency_injector-darwin-x64.dylib',
  'darwin-arm64': 'libdependency_injector-darwin-arm64.dylib',
  'win32-x64': 'dependency_injector-win32-x64.dll',
};

// Output filenames (what the library expects)
const OUTPUT_NAMES = {
  'linux': 'libdependency_injector.so',
  'darwin': 'libdependency_injector.dylib',
  'win32': 'dependency_injector.dll',
};

/**
 * Error type for checksum-verification failures. Per the failure policy
 * these are hard failures (exit 1), unlike ordinary network errors.
 *
 * Reserved for a *complete* download whose contents cannot be trusted: a
 * digest mismatch, a missing SHA256SUMS entry, or an unfetchable SHA256SUMS
 * asset. Truncated downloads are network failures and use a plain Error.
 */
class VerificationError extends Error {
  constructor(message) {
    super(message);
    this.name = 'VerificationError';
  }
}

/**
 * Get the platform key for the current system.
 */
function getPlatformKey() {
  const platform = process.platform;
  const arch = process.arch;
  return `${platform}-${arch}`;
}

/**
 * Decide how to fetch a release asset.
 *
 * Without a token, use the public `browser_download_url` (host github.com).
 * With DI_GITHUB_TOKEN, use the release-asset API endpoint (`asset.url`,
 * host api.github.com) with `Accept: application/octet-stream`: that host
 * IS the one httpsGet attaches the token to, so private-repo assets
 * authenticate. GitHub answers with a redirect to a signed CDN URL, and
 * httpsGet re-evaluates the token per hop, so the token is stripped before
 * it reaches the CDN.
 */
function assetRequest(asset) {
  if (process.env.DI_GITHUB_TOKEN && asset.url) {
    return {
      url: asset.url,
      options: { headers: { 'Accept': 'application/octet-stream' } },
    };
  }
  return { url: asset.browser_download_url, options: {} };
}

/**
 * Staging path for a download: sibling of the final path (so the rename is
 * atomic) and PID-qualified so concurrent installs cannot interleave.
 */
function stagingPathFor(outputPath) {
  return `${outputPath}.download.${process.pid}`;
}

/**
 * Make an HTTPS request with redirect following (capped at MAX_REDIRECTS).
 *
 * The DI_GITHUB_TOKEN Authorization header is attached only when the
 * request host is api.github.com; the decision is re-evaluated against
 * each redirect target's host so the token never leaks to download hosts
 * (e.g. objects.githubusercontent.com) or arbitrary redirect targets.
 */
function httpsGet(url, options = {}, redirectsLeft = MAX_REDIRECTS) {
  return new Promise((resolve, reject) => {
    const headers = {
      'User-Agent': `dependency-injector-nodejs/${PACKAGE_VERSION}`,
      ...options.headers,
    };

    if (process.env.DI_GITHUB_TOKEN && new URL(url).hostname === 'api.github.com') {
      headers['Authorization'] = `token ${process.env.DI_GITHUB_TOKEN}`;
    }

    https.get(url, { headers }, (res) => {
      // Follow redirects
      if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
        res.resume(); // Discard the redirect body and free the socket
        if (redirectsLeft <= 0) {
          reject(new Error(`Too many redirects (more than ${MAX_REDIRECTS}) while fetching ${url}`));
          return;
        }
        const target = new URL(res.headers.location, url).toString();
        httpsGet(target, options, redirectsLeft - 1).then(resolve, reject);
        return;
      }

      if (res.statusCode !== 200) {
        res.resume();
        reject(new Error(`HTTP ${res.statusCode}: ${res.statusMessage}`));
        return;
      }

      resolve(res);
    }).on('error', reject);
  });
}

/**
 * Download a file from URL to destination.
 *
 * Resolves from the write stream's 'close' event (not 'finish'), so the
 * file descriptor is closed before the caller hashes or renames the file.
 *
 * When the response advertises a Content-Length, the bytes actually written
 * are compared against it: a short read means the connection dropped
 * mid-body, which is a network failure (plain Error → soft path), not
 * tampering. VerificationError is reserved for a complete download whose
 * digest is wrong.
 */
async function downloadFile(url, dest, options = {}, getImpl = httpsGet) {
  const res = await getImpl(url, options);

  return new Promise((resolve, reject) => {
    const declared = res.headers && res.headers['content-length'];
    const expectedBytes = declared === undefined ? NaN : Number.parseInt(declared, 10);

    const file = createWriteStream(dest);
    res.pipe(file);

    file.on('close', () => {
      if (Number.isFinite(expectedBytes) && file.bytesWritten !== expectedBytes) {
        fs.rmSync(dest, { force: true }); // Clean up the truncated file
        reject(new Error(
          `incomplete download of ${url}: expected ${expectedBytes} bytes, ` +
          `received ${file.bytesWritten} (connection closed early)`
        ));
        return;
      }
      resolve();
    });
    res.on('error', (err) => {
      file.destroy();
      fs.rmSync(dest, { force: true }); // Clean up
      reject(err);
    });
    file.on('error', (err) => {
      fs.rmSync(dest, { force: true }); // Clean up
      reject(err);
    });
  });
}

/**
 * Fetch release metadata (resolved tag + asset list) for a version.
 */
async function getReleaseAssets(version) {
  const tag = version.startsWith('v') ? version : `v${version}`;
  const apiUrl = `https://api.github.com/repos/${REPO_OWNER}/${REPO_NAME}/releases/tags/${tag}`;

  const res = await httpsGet(apiUrl, {
    headers: { 'Accept': 'application/vnd.github.v3+json' },
  });

  return new Promise((resolve, reject) => {
    let data = '';
    res.on('data', chunk => data += chunk);
    res.on('end', () => {
      try {
        const release = JSON.parse(data);
        resolve({ tag, assets: release.assets || [] });
      } catch (err) {
        reject(err);
      }
    });
    res.on('error', reject);
  });
}

/**
 * Download a URL body as text (used for the SHA256SUMS asset).
 */
async function fetchText(url, options = {}) {
  const res = await httpsGet(url, options);

  return new Promise((resolve, reject) => {
    let data = '';
    res.setEncoding('utf8');
    res.on('data', chunk => data += chunk);
    res.on('end', () => resolve(data));
    res.on('error', reject);
  });
}

/**
 * Parse sha256sum output ("<hex>  <filename>", or "<hex> *<filename>" in
 * binary mode) and return the expected hash for assetName (lowercased), or
 * null if the file has no entry for it.
 */
function parseSha256Sums(text, assetName) {
  for (const rawLine of text.split('\n')) {
    const match = rawLine.trim().match(/^([0-9a-fA-F]{64})\s+\*?(.+)$/);
    if (match && match[2] === assetName) {
      return match[1].toLowerCase();
    }
  }
  return null;
}

/**
 * Hex-encoded SHA-256 digest of a Buffer or string.
 */
function sha256Hex(data) {
  return createHash('sha256').update(data).digest('hex');
}

/**
 * Hex-encoded SHA-256 digest of a file, hashed in chunks so a large library
 * is never held in memory whole (matches the Python installer's loop).
 */
function sha256File(filePath) {
  return new Promise((resolve, reject) => {
    const hash = createHash('sha256');
    const stream = createReadStream(filePath);
    stream.on('data', chunk => hash.update(chunk));
    stream.on('error', reject);
    stream.on('end', () => resolve(hash.digest('hex')));
  });
}

/**
 * Verify the downloaded file against the release's SHA256SUMS asset.
 *
 * - Release has no SHA256SUMS asset (pre-checksum release): warn loudly and
 *   continue — unless DI_REQUIRE_CHECKSUM is set, in which case throw.
 * - SHA256SUMS exists but cannot be fetched, has no entry for the asset, or
 *   the hash mismatches: throw a VerificationError (the caller deletes the
 *   staged file and exits non-zero).
 *
 * `fetchImpl` is an injection seam for tests; it defaults to fetchText and
 * receives the same (url, options) pair the real fetch would use.
 */
async function verifyChecksum(assets, tag, assetName, filePath, fetchImpl = fetchText) {
  const sumsAsset = assets.find(a => a.name === 'SHA256SUMS');

  if (!sumsAsset) {
    if (process.env.DI_REQUIRE_CHECKSUM) {
      throw new VerificationError(
        `release ${tag} has no SHA256SUMS asset and DI_REQUIRE_CHECKSUM is set`
      );
    }
    console.warn(`⚠️  WARNING: release ${tag} has no SHA256SUMS asset; skipping checksum verification (pre-checksum release). Set DI_REQUIRE_CHECKSUM=1 to make this a hard failure.`);
    return;
  }

  let sumsText;
  try {
    const { url, options } = assetRequest(sumsAsset);
    sumsText = await fetchImpl(url, options);
  } catch (err) {
    throw new VerificationError(
      `failed to download SHA256SUMS from release ${tag}: ${err.message}`
    );
  }

  const expected = parseSha256Sums(sumsText, assetName);
  if (expected === null) {
    throw new VerificationError(
      `SHA256SUMS in release ${tag} has no entry for '${assetName}'`
    );
  }

  const actual = await sha256File(filePath);
  if (actual !== expected) {
    throw new VerificationError(
      `checksum mismatch for '${assetName}' (release ${tag}): ` +
      `expected ${expected}, actual ${actual}. ` +
      'This may indicate a corrupted or tampered download.'
    );
  }

  console.log(`✅ SHA256 checksum verified: ${actual}`);
}

/**
 * Download an asset to a staging path, verify it, mark it executable and
 * move it into place. The final path only ever receives a verified file;
 * on any failure the staged file is removed and the error is re-thrown for
 * the caller to classify (VerificationError → exit 1, anything else → warn).
 *
 * `downloadImpl` / `fetchImpl` are injection seams for tests.
 */
async function stageAndInstall({
  asset,
  assets,
  tag,
  assetName,
  outputPath,
  tempPath = stagingPathFor(outputPath),
  downloadImpl = downloadFile,
  fetchImpl = fetchText,
}) {
  try {
    const { url, options } = assetRequest(asset);
    await downloadImpl(url, tempPath, options);

    // Verify against the release's SHA256SUMS BEFORE the file becomes
    // visible at its final path (throws VerificationError on failure).
    await verifyChecksum(assets, tag, assetName, tempPath, fetchImpl);

    // Make executable on Unix
    if (process.platform !== 'win32') {
      fs.chmodSync(tempPath, 0o755);
    }

    // Atomically move the verified file into place
    fs.renameSync(tempPath, outputPath);
  } catch (error) {
    // Never leave a partially-downloaded or unverified file behind
    try {
      fs.rmSync(tempPath, { force: true });
    } catch {
      // Best-effort cleanup; the file is outside the trusted final path
    }
    throw error;
  }

  return outputPath;
}

/**
 * Check if a local build exists.
 */
function findLocalBuild() {
  const outputName = OUTPUT_NAMES[process.platform];
  if (!outputName) return null;

  const possiblePaths = [
    // Project root target directory
    path.resolve(__dirname, '../../../../target/release', outputName),
    path.resolve(__dirname, '../../../target/release', outputName),
    // Custom path
    process.env.DI_LIBRARY_PATH,
  ].filter(Boolean);

  for (const p of possiblePaths) {
    if (fs.existsSync(p)) {
      return p;
    }
  }

  return null;
}

/**
 * Main installation function.
 */
async function install() {
  console.log('📦 dependency-injector: Installing native library...');

  // Check for skip flag
  if (process.env.DI_SKIP_DOWNLOAD) {
    console.log('⏭️  DI_SKIP_DOWNLOAD set, skipping download');
    return;
  }

  // Check for custom library path
  if (process.env.DI_LIBRARY_PATH) {
    if (fs.existsSync(process.env.DI_LIBRARY_PATH)) {
      console.log(`✅ Using custom library: ${process.env.DI_LIBRARY_PATH}`);
      return;
    }
    console.warn(`⚠️  DI_LIBRARY_PATH set but file not found: ${process.env.DI_LIBRARY_PATH}`);
  }

  // Check for local build
  const localBuild = findLocalBuild();
  if (localBuild) {
    console.log(`✅ Found local build: ${localBuild}`);
    return;
  }

  // Determine platform
  const platformKey = getPlatformKey();
  const assetName = PLATFORM_MAP[platformKey];

  if (!assetName) {
    console.error(`❌ Unsupported platform: ${platformKey}`);
    console.error('   Supported platforms: ' + Object.keys(PLATFORM_MAP).join(', '));
    console.error('   You can build locally with: cargo rustc --release --features ffi --crate-type cdylib');
    process.exit(1);
  }

  const outputName = OUTPUT_NAMES[process.platform];
  const outputDir = path.join(__dirname, '..', 'native');
  const outputPath = path.join(outputDir, outputName);

  // Check if already downloaded. A file at the final path was put there by a
  // completed, verified install by this version of the script (downloads are
  // staged elsewhere and only renamed in after verification). It is NOT a
  // guarantee for files left by an interrupted run of an older version that
  // downloaded straight to this path — remove `native/` if in doubt.
  if (fs.existsSync(outputPath)) {
    console.log(`✅ Native library already exists: ${outputPath}`);
    return;
  }

  // Create output directory
  fs.mkdirSync(outputDir, { recursive: true });

  // Stage the download next to the final path so the rename is atomic (same
  // filesystem), PID-qualified so concurrent installs cannot collide.
  const tempPath = stagingPathFor(outputPath);

  // Download from GitHub releases
  try {
    const { tag, assets } = await getReleaseAssets(PACKAGE_VERSION);
    console.log(`📥 Downloading asset '${assetName}' from release tag '${tag}' for ${platformKey}...`);

    const asset = assets.find(a => a.name === assetName);
    if (!asset) {
      throw new Error(`Asset '${assetName}' not found in release ${tag}`);
    }
    console.log(`   URL: ${assetRequest(asset).url}`);

    await stageAndInstall({ asset, assets, tag, assetName, outputPath, tempPath });

    console.log(`✅ Downloaded to: ${outputPath}`);
  } catch (error) {
    if (error instanceof VerificationError) {
      // Hard failure per the policy in the header comment
      console.error(`❌ Checksum verification failed: ${error.message}`);
      console.error('   The downloaded file has been deleted.');
      process.exit(1);
    }

    console.error(`❌ Failed to download native library: ${error.message}`);
    console.error('');
    console.error('You can:');
    console.error('  1. Build locally: cargo rustc --release --features ffi --crate-type cdylib');
    console.error('  2. Set DI_LIBRARY_PATH to point to an existing library');
    console.error('  3. Set DI_SKIP_DOWNLOAD=1 to skip this step');
    console.error('');

    // Don't fail the install - the library might be built later
    console.warn('⚠️  Continuing without pre-built library');
  }
}

// Run the installation only when executed directly (postinstall), not when
// imported (e.g. by the test suite).
//
// import.meta.url is symlink-resolved by Node's loader, so argv[1] must be
// realpath'd too: comparing against a merely lexical path makes the
// installer a silent no-op whenever any path component is a symlink (pnpm
// store, macOS /tmp -> /private/tmp, symlinked HOME or CI workspace).
function isMainModule() {
  if (process.argv[1] === undefined) return false;
  try {
    return __filename === fs.realpathSync(path.resolve(process.argv[1]));
  } catch {
    // argv[1] does not exist on disk (e.g. an eval'd entry point)
    return false;
  }
}

if (isMainModule()) {
  install().catch((err) => {
    console.error('Installation failed:', err);
    process.exit(1);
  });
} else {
  // Make a misdetected main module visible instead of exiting 0 silently.
  console.log(
    `ℹ️  dependency-injector install script imported (not executed as ${process.argv[1] ?? '<none>'}); skipping download.`
  );
}

// Exported for testing
export {
  assetRequest,
  downloadFile,
  parseSha256Sums,
  sha256File,
  sha256Hex,
  stageAndInstall,
  stagingPathFor,
  verifyChecksum,
  VerificationError,
};
