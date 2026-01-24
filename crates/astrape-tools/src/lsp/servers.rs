#[derive(Debug, Clone)]
pub struct LspServerConfig {
    pub name: String,
    pub command: String,
    pub extensions: Vec<String>,
    pub install_hint: String,
}

pub fn known_servers() -> Vec<LspServerConfig> {
    Vec::new()
}
