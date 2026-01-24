#!/usr/bin/env node
const fs = require('fs');
const path = require('path');
const os = require('os');

const PLUGIN_NATIVE_PATH = path.join(
  os.homedir(),
  '.claude/plugins/cache/astrape/astrape/0.1.0/native'
);
const SOURCE_PATH = __dirname.replace('/scripts', '');

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

function sync() {
  if (!fs.existsSync(PLUGIN_NATIVE_PATH)) {
    console.log('[sync-plugin] Plugin not installed, skipping');
    return;
  }

  const files = ['index.js', 'index.d.ts', getPlatformBinary()].filter(Boolean);
  
  for (const file of files) {
    const src = path.join(SOURCE_PATH, file);
    const dst = path.join(PLUGIN_NATIVE_PATH, file);
    
    if (!fs.existsSync(src)) continue;
    
    fs.copyFileSync(src, dst);
    console.log(`[sync-plugin] ${file}`);
  }
}

sync();
