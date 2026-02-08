#!/usr/bin/env node

const { execFileSync } = require('child_process');
const { existsSync } = require('fs');
const path = require('path');

const PLATFORMS = {
  'darwin-arm64': '@uiradev/uira-darwin-arm64',
  'darwin-x64': '@uiradev/uira-darwin-x64',
  'linux-x64-gnu': '@uiradev/uira-linux-x64-gnu',
  'linux-x64-musl': '@uiradev/uira-linux-x64-musl',
  'linux-arm64-gnu': '@uiradev/uira-linux-arm64-gnu',
  'win32-x64': '@uiradev/uira-win32-x64-msvc',
};

function isMusl() {
  if (process.platform !== 'linux') {
    return false;
  }
  
  try {
    const { MUSL, familySync } = require('detect-libc');
    return familySync() === MUSL;
  } catch {
    try {
      const report = process.report?.getReport();
      return report?.header?.glibcVersionRuntime === undefined;
    } catch {
      return false;
    }
  }
}

function getPlatformKey() {
  const platform = process.platform;
  const arch = process.arch;
  
  if (platform === 'linux') {
    const libc = isMusl() ? 'musl' : 'gnu';
    return `${platform}-${arch}-${libc}`;
  }
  
  return `${platform}-${arch}`;
}

function getBinaryPath() {
  const key = getPlatformKey();

  const packageName = PLATFORMS[key];
  if (!packageName) {
    console.error(`Unsupported platform: ${key}`);
    console.error('Supported platforms:', Object.keys(PLATFORMS).join(', '));
    process.exit(1);
  }

  try {
    const packagePath = require.resolve(`${packageName}/package.json`);
    const packageDir = path.dirname(packagePath);
    const binaryName = process.platform === 'win32' ? 'uira.exe' : 'uira';
    return path.join(packageDir, binaryName);
  } catch {
    return null;
  }
}

function getLocalDevBinaryPath() {
  const packageDir = path.resolve(__dirname, '..');
  const repoRoot = path.resolve(packageDir, '..', '..');
  const binaryName = process.platform === 'win32' ? 'uira.exe' : 'uira';
  const releasePath = path.join(repoRoot, 'target', 'release', binaryName);
  const debugPath = path.join(repoRoot, 'target', 'debug', binaryName);

  if (existsSync(releasePath)) {
    return releasePath;
  }

  if (existsSync(debugPath)) {
    return debugPath;
  }

  return null;
}

function canRunLocalCargo() {
  try {
    execFileSync('cargo', ['--version'], { stdio: 'ignore' });
    return true;
  } catch {
    return false;
  }
}

const args = process.argv.slice(2);
const binary = getBinaryPath();

if (binary) {
  try {
    execFileSync(binary, args, { stdio: 'inherit' });
  } catch (e) {
    if (e.status !== undefined) {
      process.exit(e.status);
    }
    throw e;
  }
  process.exit(0);
}

const localBinary = getLocalDevBinaryPath();
if (localBinary) {
  try {
    execFileSync(localBinary, args, { stdio: 'inherit' });
    process.exit(0);
  } catch (e) {
    if (e.status !== undefined) {
      process.exit(e.status);
    }
    throw e;
  }
}

if (canRunLocalCargo()) {
  try {
    execFileSync('cargo', ['run', '-p', 'uira', '--', ...args], { stdio: 'inherit' });
    process.exit(0);
  } catch (e) {
    if (e.status !== undefined) {
      process.exit(e.status);
    }
    throw e;
  }
}

const key = getPlatformKey();
const packageName = PLATFORMS[key];
console.error(`Failed to find binary package: ${packageName}`);
console.error('Try reinstalling: npm install @uiradev/uira');
process.exit(1);
