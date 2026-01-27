use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use uira_sdk::AgentConfig;

pub type AgentFactory = Arc<dyn Fn() -> AgentConfig + Send + Sync + 'static>;

#[derive(Default, Clone)]
pub struct AgentRegistry {
    factories: HashMap<String, AgentFactory>,
}

impl fmt::Debug for AgentRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AgentRegistry")
            .field("agents", &self.names())
            .finish()
    }
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_factory(&mut self, name: impl Into<String>, factory: AgentFactory) {
        self.factories.insert(name.into(), factory);
    }

    pub fn register_config(&mut self, config: AgentConfig) {
        let name = config.name.clone();
        self.register_factory(name, Arc::new(move || config.clone()));
    }

    pub fn get(&self, name: &str) -> Option<AgentConfig> {
        self.factories.get(name).map(|f| (f)())
    }

    pub fn contains(&self, name: &str) -> bool {
        self.factories.contains_key(name)
    }

    pub fn names(&self) -> Vec<String> {
        let mut out: Vec<_> = self.factories.keys().cloned().collect();
        out.sort();
        out
    }

    pub fn to_configs(&self) -> HashMap<String, AgentConfig> {
        self.factories
            .iter()
            .map(|(k, f)| (k.clone(), (f)()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_roundtrip() {
        let mut reg = AgentRegistry::new();

        reg.register_config(AgentConfig {
            name: "explore".to_string(),
            description: "".to_string(),
            prompt: "".to_string(),
            tools: vec!["Read".to_string()],
            model: None,
            default_model: None,
            metadata: None,
        });

        assert!(reg.contains("explore"));
        assert_eq!(reg.get("explore").unwrap().tools.len(), 1);
        assert!(reg.get("missing").is_none());
    }
}
