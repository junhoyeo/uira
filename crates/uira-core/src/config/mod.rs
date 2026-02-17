pub mod loader;
pub mod schema;

pub use loader::{
    find_all_config_files, load_config, load_config_from_file, resolve_config, ConfigFormat,
    ResolvedConfig,
};
pub use schema::{
    AgentConfig, AgentSettings, AiHookCommand, AiHooksConfig, AnthropicProviderSettings,
    CommentsAiSettings, CommentsSettings, DiagnosticsAiSettings, DiagnosticsSettings, HookCommand,
    HookConfig, HooksConfig, KeybindsConfig, McpServerConfig, McpSettings, NamedMcpServerConfig,
    PayloadLogSettings, ProvidersSettings, SidebarConfig, ThemeColorOverrides, TyposAiSettings,
    TyposSettings, UiraConfig,
};
