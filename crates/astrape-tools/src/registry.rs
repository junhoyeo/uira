use crate::types::{ToolDefinition, ToolError};
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct ToolRegistry {
    tools: HashMap<String, ToolDefinition>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn register(&mut self, tool: ToolDefinition) -> Result<(), ToolError> {
        let name = tool.name.clone();
        if self.tools.contains_key(&name) {
            return Err(ToolError::AlreadyRegistered { name });
        }
        self.tools.insert(name, tool);
        Ok(())
    }

    pub fn register_many(
        &mut self,
        tools: impl IntoIterator<Item = ToolDefinition>,
    ) -> Result<(), ToolError> {
        for tool in tools {
            self.register(tool)?;
        }
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&ToolDefinition> {
        self.tools.get(name)
    }

    pub fn get_required(&self, name: &str) -> Result<&ToolDefinition, ToolError> {
        self.tools.get(name).ok_or_else(|| ToolError::NotFound {
            name: name.to_string(),
        })
    }

    pub fn names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.tools.keys().map(String::as_str).collect();
        names.sort_unstable();
        names
    }

    pub fn len(&self) -> usize {
        self.tools.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ToolOutput;
    use serde_json::json;
    use std::sync::Arc;

    fn tool(name: &str) -> ToolDefinition {
        ToolDefinition::new(
            name,
            "desc",
            json!({"type": "object", "properties": {}, "required": []}),
            Arc::new(|_input| async { Ok(ToolOutput::text("ok")) }),
        )
    }

    #[test]
    fn register_and_get() {
        let mut r = ToolRegistry::new();
        r.register(tool("a")).unwrap();
        assert!(r.get("a").is_some());
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn register_duplicate_rejected() {
        let mut r = ToolRegistry::new();
        r.register(tool("a")).unwrap();
        let err = r.register(tool("a")).unwrap_err();
        assert_eq!(
            err,
            ToolError::AlreadyRegistered {
                name: "a".to_string()
            }
        );
    }

    #[test]
    fn names_are_sorted() {
        let mut r = ToolRegistry::new();
        r.register(tool("b")).unwrap();
        r.register(tool("a")).unwrap();
        assert_eq!(r.names(), vec!["a", "b"]);
    }

    #[test]
    fn get_required_missing_returns_not_found() {
        let r = ToolRegistry::new();
        let err = r.get_required("missing").unwrap_err();
        assert_eq!(
            err,
            ToolError::NotFound {
                name: "missing".to_string()
            }
        );
    }
}
