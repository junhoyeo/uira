#!/usr/bin/env node
const fs = require('fs');
const path = require('path');
const os = require('os');

const SOURCE_PATH = path.resolve(__dirname, '..');
const PLUGIN_PKG_PATH = path.resolve(__dirname, '../../../packages/claude-plugin/native');
const PLUGIN_PKG_JSON = path.resolve(__dirname, '../../../packages/claude-plugin/package.json');

function getPluginCachePath() {
  try {
    const pkg = JSON.parse(fs.readFileSync(PLUGIN_PKG_JSON, 'utf8'));
    return path.join(os.homedir(), `.claude/plugins/cache/astrape/astrape/${pkg.version}/native`);
  } catch {
    return path.join(os.homedir(), '.claude/plugins/cache/astrape/astrape/0.1.0/native');
  }
}

function getPlatformBinary() {
  const map = {
    'darwin-arm64': 'astrape.darwin-arm64.node',
    'darwin-x64': 'astrape.darwin-x64.node',
    'linux-x64': 'astrape.linux-x64-gnu.node',
    'linux-arm64': 'astrape.linux-arm64-gnu.node',
    'win32-x64': 'astrape.win32-x64-msvc.node',
  };
  return map[`${os.platform()}-${os.arch()}`];
}

function syncTo(targetPath, label) {
  if (!fs.existsSync(targetPath)) return;

  const files = ['index.js', 'index.d.ts', getPlatformBinary()].filter(Boolean);
  
  for (const file of files) {
    const src = path.join(SOURCE_PATH, file);
    const dst = path.join(targetPath, file);
    if (!fs.existsSync(src)) continue;
    fs.copyFileSync(src, dst);
  }
  console.log(`[sync] ${label}`);
}

syncTo(PLUGIN_PKG_PATH, 'packages/claude-plugin/native');
syncTo(getPluginCachePath(), '~/.claude/plugins/cache (installed)');
