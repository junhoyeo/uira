export declare const pluginPath: string;
export declare const pluginConfigPath: string;
export declare const agentsPath: string;
export declare const hooksPath: string;
export declare const commandsPath: string;
export declare const templatesPath: string;
export declare const claudeConfigPath: string;
export declare const mcpConfigPath: string;

export interface PluginConfig {
  name: string;
  version: string;
  description: string;
}

export interface McpConfig {
  mcpServers?: Record<string, unknown>;
}

export declare function getPluginConfig(): PluginConfig;
export declare function getMcpConfig(): McpConfig;

declare const plugin: {
  pluginPath: string;
  pluginConfigPath: string;
  agentsPath: string;
  hooksPath: string;
  commandsPath: string;
  templatesPath: string;
  claudeConfigPath: string;
  mcpConfigPath: string;
  getPluginConfig: typeof getPluginConfig;
  getMcpConfig: typeof getMcpConfig;
};

export default plugin;
