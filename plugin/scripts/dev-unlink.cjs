#!/usr/bin/env node
/**
 * Disable development mode: Remove symlinks and restore original files
 */

const fs = require('fs');
const path = require('path');
const os = require('os');

const PLUGIN_CACHE_PATH = path.join(
  os.homedir(),
  '.claude/plugins/cache/astrape/astrape/0.1.0/native'
);
const SOURCE_PATH = path.resolve(__dirname, '../../crates/astrape-napi');

function getPlatformBinaryName() {
  const platform = os.platform();
  const arch = os.arch();
  const platformMap = {
    darwin: { arm64: 'darwin-arm64', x64: 'darwin-x64' },
    linux: { x64: 'linux-x64-gnu', arm64: 'linux-arm64-gnu' },
    win32: { x64: 'win32-x64-msvc', arm64: 'win32-arm64-msvc' },
  };
  const target = platformMap[platform]?.[arch];
  return target ? `astrape.${target}.node` : null;
}

function restoreFile(targetPath) {
  const backupPath = `${targetPath}.backup`;
  
  if (!fs.existsSync(targetPath)) {
    console.log(`  SKIP: ${path.basename(targetPath)} (not found)`);
    return;
  }

  const stats = fs.lstatSync(targetPath);
  if (!stats.isSymbolicLink()) {
    console.log(`  SKIP: ${path.basename(targetPath)} (not a symlink)`);
    return;
  }

  fs.unlinkSync(targetPath);
  console.log(`  Removed symlink: ${path.basename(targetPath)}`);

  if (fs.existsSync(backupPath)) {
    fs.renameSync(backupPath, targetPath);
    console.log(`  Restored backup: ${path.basename(targetPath)}`);
  } else {
    const sourcePath = path.join(SOURCE_PATH, path.basename(targetPath));
    if (fs.existsSync(sourcePath)) {
      fs.copyFileSync(sourcePath, targetPath);
      console.log(`  Copied from source: ${path.basename(targetPath)}`);
    }
  }
}

function main() {
  console.log('=== Astrape Dev Mode Disable ===\n');
  console.log(`Target: ${PLUGIN_CACHE_PATH}\n`);

  if (!fs.existsSync(PLUGIN_CACHE_PATH)) {
    console.error('ERROR: Plugin not installed');
    process.exit(1);
  }

  console.log('Removing symlinks...');
  
  restoreFile(path.join(PLUGIN_CACHE_PATH, 'index.js'));
  restoreFile(path.join(PLUGIN_CACHE_PATH, 'index.d.ts'));
  
  const binaryName = getPlatformBinaryName();
  if (binaryName) {
    restoreFile(path.join(PLUGIN_CACHE_PATH, binaryName));
  }

  console.log('\n=== Dev Mode Disabled ===');
}

main();
