#!/usr/bin/env node

const { execFileSync } = require('child_process');
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
  } catch (e) {
    console.error(`Failed to find binary package: ${packageName}`);
    console.error('Try reinstalling: npm install @uiradev/uira');
    process.exit(1);
  }
}

const binary = getBinaryPath();
const args = process.argv.slice(2);

try {
  execFileSync(binary, args, { stdio: 'inherit' });
} catch (e) {
  if (e.status !== undefined) {
    process.exit(e.status);
  }
  throw e;
}
