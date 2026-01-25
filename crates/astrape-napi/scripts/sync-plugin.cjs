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
  const platform = os.platform();
  const arch = os.arch();
  const key = `${platform}-${arch}`;

  // Direct mappings for non-Linux platforms
  const directMap = {
    'darwin-arm64': 'astrape.darwin-arm64.node',
    'darwin-x64': 'astrape.darwin-x64.node',
    'win32-x64': 'astrape.win32-x64-msvc.node',
  };

  if (directMap[key]) {
    return directMap[key];
  }

  // For Linux, check which binary exists (GNU vs MUSL)
  if (platform === 'linux') {
    const archMap = { 'x64': 'x64', 'arm64': 'arm64' };
    const nodeArch = archMap[arch];
    if (!nodeArch) return undefined;

    const muslBinary = `astrape.linux-${nodeArch}-musl.node`;
    const gnuBinary = `astrape.linux-${nodeArch}-gnu.node`;

    // Prefer the binary that exists; check MUSL first (Alpine/Docker common case)
    if (fs.existsSync(path.join(SOURCE_PATH, muslBinary))) {
      return muslBinary;
    }
    if (fs.existsSync(path.join(SOURCE_PATH, gnuBinary))) {
      return gnuBinary;
    }
  }

  return undefined;
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

// Sync agent descriptions with configured models
require('./sync-agent-descriptions.cjs');

// Also sync agents directory to cache
const AGENTS_SRC = path.resolve(__dirname, '../../../packages/claude-plugin/agents');
const AGENTS_CACHE = getPluginCachePath().replace('/native', '/agents');
if (fs.existsSync(AGENTS_SRC) && fs.existsSync(path.dirname(AGENTS_CACHE))) {
  const agentFiles = fs.readdirSync(AGENTS_SRC).filter(f => f.endsWith('.md'));
  if (!fs.existsSync(AGENTS_CACHE)) {
    fs.mkdirSync(AGENTS_CACHE, { recursive: true });
  }
  for (const file of agentFiles) {
    fs.copyFileSync(path.join(AGENTS_SRC, file), path.join(AGENTS_CACHE, file));
  }
  console.log(`[sync] agents → ~/.claude/plugins/cache (${agentFiles.length} files)`);
}
