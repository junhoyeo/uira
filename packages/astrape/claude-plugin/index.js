import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { readFileSync } from 'node:fs';

const __dirname = dirname(fileURLToPath(import.meta.url));

export const pluginPath = __dirname;
export const pluginConfigPath = join(__dirname, '.claude-plugin', 'plugin.json');
export const agentsPath = join(__dirname, 'agents');
export const hooksPath = join(__dirname, 'hooks');
export const commandsPath = join(__dirname, 'commands');
export const templatesPath = join(__dirname, 'templates');
export const claudeConfigPath = join(__dirname, 'CLAUDE.md');
export const mcpConfigPath = join(__dirname, '.mcp.json');

let _pluginConfig = null;
export function getPluginConfig() {
  if (!_pluginConfig) {
    _pluginConfig = JSON.parse(readFileSync(pluginConfigPath, 'utf-8'));
  }
  return _pluginConfig;
}

let _mcpConfig = null;
export function getMcpConfig() {
  if (!_mcpConfig) {
    _mcpConfig = JSON.parse(readFileSync(mcpConfigPath, 'utf-8'));
  }
  return _mcpConfig;
}

export default {
  pluginPath,
  pluginConfigPath,
  agentsPath,
  hooksPath,
  commandsPath,
  templatesPath,
  claudeConfigPath,
  mcpConfigPath,
  getPluginConfig,
  getMcpConfig,
};
