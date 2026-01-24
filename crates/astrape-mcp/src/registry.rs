use std::collections::HashMap;

use crate::types::McpServerConfig;

/// Registry for managing MCP servers
#[derive(Debug, Clone)]
pub struct McpServerRegistry {
    servers: HashMap<String, McpServerConfig>,
}

impl McpServerRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            servers: HashMap::new(),
        }
    }

    /// Register a new MCP server
    pub fn register(&mut self, config: McpServerConfig) {
        self.servers.insert(config.name.clone(), config);
    }

    /// Unregister an MCP server
    pub fn unregister(&mut self, name: &str) -> Option<McpServerConfig> {
        self.servers.remove(name)
    }

    /// Get a server configuration by name
    pub fn get(&self, name: &str) -> Option<&McpServerConfig> {
        self.servers.get(name)
    }

    /// Get a mutable reference to a server configuration
    pub fn get_mut(&mut self, name: &str) -> Option<&mut McpServerConfig> {
        self.servers.get_mut(name)
    }

    /// List all registered servers
    pub fn list(&self) -> Vec<&McpServerConfig> {
        self.servers.values().collect()
    }

    /// Check if a server is registered
    pub fn contains(&self, name: &str) -> bool {
        self.servers.contains_key(name)
    }

    /// Get the number of registered servers
    pub fn len(&self) -> usize {
        self.servers.len()
    }

    /// Check if the registry is empty
    pub fn is_empty(&self) -> bool {
        self.servers.is_empty()
    }

    /// Clear all registered servers
    pub fn clear(&mut self) {
        self.servers.clear();
    }

    /// Load servers from a list of configurations
    pub fn load_from_configs(&mut self, configs: Vec<McpServerConfig>) {
        for config in configs {
            self.register(config);
        }
    }

    /// Export all server configurations
    pub fn export_configs(&self) -> Vec<McpServerConfig> {
        self.servers.values().cloned().collect()
    }
}

impl Default for McpServerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config(name: &str) -> McpServerConfig {
        McpServerConfig {
            name: name.to_string(),
            command: format!("mcp-{}", name),
            args: vec![],
            env: None,
        }
    }

    #[test]
    fn test_registry_new() {
        let registry = McpServerRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_registry_register() {
        let mut registry = McpServerRegistry::new();
        let config = create_test_config("test-server");

        registry.register(config);
        assert_eq!(registry.len(), 1);
        assert!(registry.contains("test-server"));
    }

    #[test]
    fn test_registry_get() {
        let mut registry = McpServerRegistry::new();
        let config = create_test_config("test-server");

        registry.register(config.clone());
        let retrieved = registry.get("test-server").unwrap();

        assert_eq!(retrieved.name, "test-server");
        assert_eq!(retrieved.command, "mcp-test-server");
    }

    #[test]
    fn test_registry_unregister() {
        let mut registry = McpServerRegistry::new();
        let config = create_test_config("test-server");

        registry.register(config);
        assert_eq!(registry.len(), 1);

        let removed = registry.unregister("test-server");
        assert!(removed.is_some());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_registry_list() {
        let mut registry = McpServerRegistry::new();

        registry.register(create_test_config("server1"));
        registry.register(create_test_config("server2"));
        registry.register(create_test_config("server3"));

        let list = registry.list();
        assert_eq!(list.len(), 3);
    }

    #[test]
    fn test_registry_clear() {
        let mut registry = McpServerRegistry::new();

        registry.register(create_test_config("server1"));
        registry.register(create_test_config("server2"));
        assert_eq!(registry.len(), 2);

        registry.clear();
        assert!(registry.is_empty());
    }

    #[test]
    fn test_registry_load_from_configs() {
        let mut registry = McpServerRegistry::new();
        let configs = vec![
            create_test_config("server1"),
            create_test_config("server2"),
            create_test_config("server3"),
        ];

        registry.load_from_configs(configs);
        assert_eq!(registry.len(), 3);
    }

    #[test]
    fn test_registry_export_configs() {
        let mut registry = McpServerRegistry::new();

        registry.register(create_test_config("server1"));
        registry.register(create_test_config("server2"));

        let exported = registry.export_configs();
        assert_eq!(exported.len(), 2);
    }

    #[test]
    fn test_registry_get_mut() {
        let mut registry = McpServerRegistry::new();
        let config = create_test_config("test-server");

        registry.register(config);

        if let Some(server) = registry.get_mut("test-server") {
            server.args.push("--debug".to_string());
        }

        let retrieved = registry.get("test-server").unwrap();
        assert_eq!(retrieved.args.len(), 1);
        assert_eq!(retrieved.args[0], "--debug");
    }

    #[test]
    fn test_registry_default() {
        let registry = McpServerRegistry::default();
        assert!(registry.is_empty());
    }
}
