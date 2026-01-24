#!/usr/bin/env node
/**
 * Development mode: Create symlinks from installed plugin to source files
 * This allows changes in crates/astrape-napi to be reflected immediately
 */

const fs = require('fs');
const path = require('path');
const os = require('os');

const PLUGIN_CACHE_PATH = path.join(
  os.homedir(),
  '.claude/plugins/cache/astrape/astrape/0.1.0/native'
);
const SOURCE_PATH = path.resolve(__dirname, '../../crates/astrape-napi');

const FILES_TO_LINK = ['index.js', 'index.d.ts'];

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

function createSymlink(source, target) {
  if (fs.existsSync(target)) {
    const stats = fs.lstatSync(target);
    if (stats.isSymbolicLink()) {
      console.log(`  Removing existing symlink: ${path.basename(target)}`);
      fs.unlinkSync(target);
    } else {
      console.log(`  Backing up existing file: ${path.basename(target)}`);
      fs.renameSync(target, `${target}.backup`);
    }
  }
  
  fs.symlinkSync(source, target);
  console.log(`  Linked: ${path.basename(target)} -> ${source}`);
}

function main() {
  console.log('=== Astrape Dev Mode Setup ===\n');
  console.log(`Source: ${SOURCE_PATH}`);
  console.log(`Target: ${PLUGIN_CACHE_PATH}\n`);

  if (!fs.existsSync(PLUGIN_CACHE_PATH)) {
    console.error('ERROR: Plugin not installed. Run: claude plugin add astrape');
    process.exit(1);
  }

  if (!fs.existsSync(SOURCE_PATH)) {
    console.error('ERROR: Source path not found');
    process.exit(1);
  }

  console.log('Creating symlinks...');
  
  for (const file of FILES_TO_LINK) {
    const source = path.join(SOURCE_PATH, file);
    const target = path.join(PLUGIN_CACHE_PATH, file);
    
    if (!fs.existsSync(source)) {
      console.log(`  SKIP: ${file} (not found in source)`);
      continue;
    }
    
    createSymlink(source, target);
  }

  const binaryName = getPlatformBinaryName();
  if (binaryName) {
    const source = path.join(SOURCE_PATH, binaryName);
    const target = path.join(PLUGIN_CACHE_PATH, binaryName);
    
    if (fs.existsSync(source)) {
      createSymlink(source, target);
    } else {
      console.log(`  SKIP: ${binaryName} (build first with: cd crates/astrape-napi && bun run build)`);
    }
  }

  console.log('\n=== Dev Mode Enabled ===');
  console.log('Changes to crates/astrape-napi will now be reflected in the plugin.');
  console.log('To disable: bun run dev:unlink');
}

main();
