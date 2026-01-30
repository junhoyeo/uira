pub mod loader;
pub mod schema;

pub use loader::{
    find_all_config_files, load_config, load_config_from_file, resolve_config, ConfigFormat,
    ResolvedConfig,
};
pub use schema::{
    AgentConfig, AgentSettings, AiHookCommand, AiHooksConfig, CommentsAiSettings, CommentsSettings,
    DiagnosticsAiSettings, DiagnosticsSettings, HookCommand, HookConfig, HooksConfig,
    McpServerConfig, McpSettings, TyposAiSettings, TyposSettings, UiraConfig,
};
