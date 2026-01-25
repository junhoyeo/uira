#!/usr/bin/env node
import { spawn } from 'node:child_process';
import { existsSync, chmodSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));

/**
 * Get the platform-specific binary name
 */
function getPlatformBinary() {
  const platform = process.platform;
  const arch = process.arch;
  const ext = platform === 'win32' ? '.exe' : '';
  
  const platformMap = {
    'darwin-arm64': 'astrape-darwin-arm64',
    'darwin-x64': 'astrape-darwin-x64',
    'linux-x64': 'astrape-linux-x64-gnu',
    'linux-arm64': 'astrape-linux-arm64-gnu',
    'win32-x64': 'astrape-win32-x64-msvc',
  };
  
  const key = `${platform}-${arch}`;
  const binaryName = platformMap[key];
  
  if (!binaryName) {
    return null;
  }
  
  return binaryName + ext;
}

/**
 * Find the binary path - check multiple locations
 */
function findBinaryPath() {
  const binaryName = getPlatformBinary();
  
  if (!binaryName) {
    throw new Error(`Unsupported platform: ${process.platform}-${process.arch}`);
  }
  
  // 1. Check in .binary directory (downloaded by postinstall)
  const binaryDir = join(__dirname, '.binary');
  const downloadedPath = join(binaryDir, binaryName);
  if (existsSync(downloadedPath)) {
    return downloadedPath;
  }
  
  // 2. Check for development binary (local cargo build)
  const devBinaryPath = join(__dirname, '..', '..', '..', 'target', 'release', 'astrape');
  if (existsSync(devBinaryPath)) {
    return devBinaryPath;
  }
  
  // 3. Check debug build
  const debugBinaryPath = join(__dirname, '..', '..', '..', 'target', 'debug', 'astrape');
  if (existsSync(debugBinaryPath)) {
    return debugBinaryPath;
  }
  
  throw new Error(
    `Astrape binary not found.\n` +
    `Expected: ${downloadedPath}\n\n` +
    `Run 'npm rebuild astrape' to download the binary, or build from source:\n` +
    `  cargo build --release -p astrape`
  );
}

// Find and execute the binary
try {
  const binaryPath = findBinaryPath();
  
  // Ensure binary is executable (Unix only)
  if (process.platform !== 'win32') {
    try {
      chmodSync(binaryPath, 0o755);
    } catch {
      // Ignore permission errors - might already be executable
    }
  }
  
  const child = spawn(binaryPath, process.argv.slice(2), {
    stdio: 'inherit',
    env: process.env,
  });
  
  child.on('error', (err) => {
    console.error(`Failed to execute astrape: ${err.message}`);
    process.exit(1);
  });
  
  child.on('exit', (code, signal) => {
    if (signal) {
      process.exit(128 + (signal === 'SIGINT' ? 2 : 15));
    }
    process.exit(code ?? 0);
  });
} catch (err) {
  console.error(err.message);
  process.exit(1);
}
