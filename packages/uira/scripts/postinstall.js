#!/usr/bin/env node
import { existsSync, mkdirSync, createWriteStream, chmodSync, unlinkSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { get } from 'node:https';
import { createGunzip } from 'node:zlib';

const __dirname = dirname(fileURLToPath(import.meta.url));

// Configuration
const VERSION = process.env.UIRA_VERSION || '0.2.0';
const GITHUB_REPO = 'junhoyeo/uira';
const GITHUB_RELEASE_URL = `https://github.com/${GITHUB_REPO}/releases/download/v${VERSION}`;
const BINARY_DIR = join(__dirname, '..', 'bin', '.binary');

/**
 * Get platform-specific binary name
 */
function getPlatformBinary() {
  const platform = process.platform;
  const arch = process.arch;
  
  const platformMap = {
    'darwin-arm64': 'uira-darwin-arm64',
    'darwin-x64': 'uira-darwin-x64',
    'linux-x64': 'uira-linux-x64-gnu',
    'linux-arm64': 'uira-linux-arm64-gnu',
    'win32-x64': 'uira-win32-x64-msvc.exe',
  };
  
  const key = `${platform}-${arch}`;
  return platformMap[key] || null;
}

/**
 * Download a file with redirect following
 */
function download(url, destPath, maxRedirects = 5) {
  return new Promise((resolve, reject) => {
    if (maxRedirects <= 0) {
      reject(new Error('Too many redirects'));
      return;
    }
    
    get(url, (response) => {
      // Handle redirects
      if (response.statusCode >= 300 && response.statusCode < 400 && response.headers.location) {
        const redirectUrl = response.headers.location;
        download(redirectUrl, destPath, maxRedirects - 1).then(resolve).catch(reject);
        return;
      }
      
      if (response.statusCode !== 200) {
        reject(new Error(`Failed to download: HTTP ${response.statusCode}`));
        return;
      }
      
      const file = createWriteStream(destPath);
      response.pipe(file);
      
      file.on('finish', () => {
        file.close(resolve);
      });
      
      file.on('error', (err) => {
        unlinkSync(destPath);
        reject(err);
      });
    }).on('error', reject);
  });
}

/**
 * Main postinstall function
 */
async function main() {
  const binaryName = getPlatformBinary();
  
  if (!binaryName) {
    console.log(`[uira] No prebuilt binary for ${process.platform}-${process.arch}`);
    console.log('[uira] You can build from source: cargo build --release -p uira');
    return;
  }
  
  const binaryPath = join(BINARY_DIR, binaryName);
  
  // Check if already downloaded
  if (existsSync(binaryPath)) {
    console.log('[uira] Binary already exists, skipping download');
    return;
  }
  
  // Create binary directory
  mkdirSync(BINARY_DIR, { recursive: true });
  
  const downloadUrl = `${GITHUB_RELEASE_URL}/${binaryName}`;
  
  console.log(`[uira] Downloading CLI binary for ${process.platform}-${process.arch}...`);
  console.log(`[uira] URL: ${downloadUrl}`);
  
  try {
    await download(downloadUrl, binaryPath);
    
    // Make executable on Unix
    if (process.platform !== 'win32') {
      chmodSync(binaryPath, 0o755);
    }
    
    console.log('[uira] Binary installed successfully');
  } catch (err) {
    console.warn(`[uira] Failed to download binary: ${err.message}`);
    console.warn('[uira] You can build from source: cargo build --release -p uira');
    console.warn('[uira] Or download manually from:', downloadUrl);
    
    // Clean up partial download
    if (existsSync(binaryPath)) {
      try {
        unlinkSync(binaryPath);
      } catch {
        // Ignore cleanup errors
      }
    }
    
    // Don't fail the install - binary is optional
  }
}

main().catch((err) => {
  console.warn('[uira] Postinstall warning:', err.message);
  // Don't exit with error - allow install to continue
});
