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
 * - DI_GITHUB_TOKEN: GitHub token for private repos or rate limiting
 */

import https from 'https';
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import { createWriteStream } from 'fs';
import { createHash } from 'node:crypto';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Configuration
const REPO_OWNER = 'pegasusheavy';
const REPO_NAME = 'dependency-injector';
const PACKAGE_VERSION = JSON.parse(
  fs.readFileSync(path.join(__dirname, '..', 'package.json'), 'utf8')
).version;

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
 * Get the platform key for the current system.
 */
function getPlatformKey() {
  const platform = process.platform;
  const arch = process.arch;
  return `${platform}-${arch}`;
}

/**
 * Make an HTTPS request with redirect following.
 */
function httpsGet(url, options = {}) {
  return new Promise((resolve, reject) => {
    const headers = {
      'User-Agent': `dependency-injector-nodejs/${PACKAGE_VERSION}`,
      ...options.headers,
    };

    if (process.env.DI_GITHUB_TOKEN) {
      headers['Authorization'] = `token ${process.env.DI_GITHUB_TOKEN}`;
    }

    https.get(url, { headers }, (res) => {
      // Follow redirects
      if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
        return httpsGet(res.headers.location, options).then(resolve).catch(reject);
      }

      if (res.statusCode !== 200) {
        reject(new Error(`HTTP ${res.statusCode}: ${res.statusMessage}`));
        return;
      }

      resolve(res);
    }).on('error', reject);
  });
}

/**
 * Download a file from URL to destination.
 */
async function downloadFile(url, dest) {
  const res = await httpsGet(url);

  return new Promise((resolve, reject) => {
    const file = createWriteStream(dest);
    res.pipe(file);
    file.on('finish', () => {
      file.close();
      resolve();
    });
    file.on('error', (err) => {
      fs.unlink(dest, () => {}); // Clean up
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
  });
}

/**
 * Download a URL body as text (used for the SHA256SUMS asset).
 */
async function fetchText(url) {
  const res = await httpsGet(url);

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
 * binary mode) and return the expected hash for assetName, or null if the
 * file has no entry for it.
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
 * Verify the downloaded file against the release's SHA256SUMS asset.
 *
 * - Release has no SHA256SUMS asset (pre-checksum release): warn loudly and
 *   continue.
 * - SHA256SUMS exists but cannot be fetched, has no entry for the asset, or
 *   the hash mismatches: delete the downloaded file and exit non-zero.
 */
async function verifyChecksum(assets, tag, assetName, filePath) {
  const sumsAsset = assets.find(a => a.name === 'SHA256SUMS');

  if (!sumsAsset) {
    console.warn(`⚠️  WARNING: release ${tag} has no SHA256SUMS asset; skipping checksum verification (pre-checksum release).`);
    return;
  }

  let sumsText;
  try {
    sumsText = await fetchText(sumsAsset.browser_download_url);
  } catch (err) {
    fs.unlinkSync(filePath);
    console.error(`❌ Failed to download SHA256SUMS from release ${tag}: ${err.message}`);
    console.error('   Deleted the downloaded library since it could not be verified.');
    process.exit(1);
  }

  const expected = parseSha256Sums(sumsText, assetName);
  if (!expected) {
    fs.unlinkSync(filePath);
    console.error(`❌ SHA256SUMS in release ${tag} has no entry for '${assetName}'. Deleted the downloaded library.`);
    process.exit(1);
  }

  const actual = createHash('sha256').update(fs.readFileSync(filePath)).digest('hex');
  if (actual !== expected) {
    fs.unlinkSync(filePath);
    console.error(`❌ Checksum mismatch for '${assetName}' (release ${tag}):`);
    console.error(`   expected ${expected}`);
    console.error(`   actual   ${actual}`);
    console.error('   The downloaded library has been deleted. This may indicate a corrupted or tampered download.');
    process.exit(1);
  }

  console.log(`✅ SHA256 checksum verified: ${actual}`);
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

  // Check if already downloaded
  if (fs.existsSync(outputPath)) {
    console.log(`✅ Native library already exists: ${outputPath}`);
    return;
  }

  // Create output directory
  fs.mkdirSync(outputDir, { recursive: true });

  // Download from GitHub releases
  try {
    const { tag, assets } = await getReleaseAssets(PACKAGE_VERSION);
    console.log(`📥 Downloading asset '${assetName}' from release tag '${tag}' for ${platformKey}...`);

    const asset = assets.find(a => a.name === assetName);
    if (!asset) {
      throw new Error(`Asset '${assetName}' not found in release ${tag}`);
    }
    console.log(`   URL: ${asset.browser_download_url}`);

    await downloadFile(asset.browser_download_url, outputPath);

    // Make executable on Unix
    if (process.platform !== 'win32') {
      fs.chmodSync(outputPath, 0o755);
    }

    // Verify against the release's SHA256SUMS (exits on mismatch)
    await verifyChecksum(assets, tag, assetName, outputPath);

    console.log(`✅ Downloaded to: ${outputPath}`);
  } catch (error) {
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

// Run installation
install().catch((err) => {
  console.error('Installation failed:', err);
  process.exit(1);
});

