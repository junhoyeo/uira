//! Tool-related types for the protocol

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// JSON Schema for a tool's input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonSchema {
    #[serde(rename = "type")]
    pub schema_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "additionalProperties"
    )]
    pub additional_properties: Option<bool>,
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, Value>,
}

impl JsonSchema {
    pub fn object() -> Self {
        Self {
            schema_type: "object".to_string(),
            description: None,
            properties: Some(serde_json::json!({})),
            required: None,
            additional_properties: Some(false),
            extra: std::collections::HashMap::new(),
        }
    }

    pub fn string() -> Self {
        Self {
            schema_type: "string".to_string(),
            description: None,
            properties: None,
            required: None,
            additional_properties: None,
            extra: std::collections::HashMap::new(),
        }
    }

    pub fn number() -> Self {
        Self {
            schema_type: "number".to_string(),
            description: None,
            properties: None,
            required: None,
            additional_properties: None,
            extra: std::collections::HashMap::new(),
        }
    }

    pub fn boolean() -> Self {
        Self {
            schema_type: "boolean".to_string(),
            description: None,
            properties: None,
            required: None,
            additional_properties: None,
            extra: std::collections::HashMap::new(),
        }
    }

    pub fn array(items: JsonSchema) -> Self {
        let mut extra = std::collections::HashMap::new();
        extra.insert("items".to_string(), serde_json::to_value(items).unwrap());
        Self {
            schema_type: "array".to_string(),
            description: None,
            properties: None,
            required: None,
            additional_properties: None,
            extra,
        }
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn property(mut self, name: &str, schema: JsonSchema) -> Self {
        let props = self.properties.get_or_insert(serde_json::json!({}));
        if let Some(obj) = props.as_object_mut() {
            obj.insert(name.to_string(), serde_json::to_value(schema).unwrap());
        }
        self
    }

    pub fn with_properties(mut self, properties: Value) -> Self {
        self.properties = Some(properties);
        self
    }

    pub fn required(mut self, fields: &[&str]) -> Self {
        self.required = Some(fields.iter().map(|s| s.to_string()).collect());
        self
    }

    pub fn with_required(mut self, required: Vec<String>) -> Self {
        self.required = Some(required);
        self
    }
}

/// Tool specification for model API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: JsonSchema,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

impl ToolSpec {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        schema: JsonSchema,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema: schema,
            cache_control: None,
        }
    }

    pub fn with_cache(mut self) -> Self {
        self.cache_control = Some(CacheControl::ephemeral());
        self
    }
}

/// Cache control for prompt caching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheControl {
    #[serde(rename = "type")]
    pub control_type: String,
}

impl CacheControl {
    pub fn ephemeral() -> Self {
        Self {
            control_type: "ephemeral".to_string(),
        }
    }
}

/// Approval requirement for a tool
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ApprovalRequirement {
    /// Skip approval, optionally bypass sandbox
    Skip {
        #[serde(default)]
        bypass_sandbox: bool,
    },
    /// Requires user approval
    NeedsApproval { reason: String },
    /// Always forbidden
    Forbidden { reason: String },
}

impl ApprovalRequirement {
    pub fn skip() -> Self {
        Self::Skip {
            bypass_sandbox: false,
        }
    }

    pub fn skip_bypass_sandbox() -> Self {
        Self::Skip {
            bypass_sandbox: true,
        }
    }

    pub fn needs_approval(reason: impl Into<String>) -> Self {
        Self::NeedsApproval {
            reason: reason.into(),
        }
    }

    pub fn forbidden(reason: impl Into<String>) -> Self {
        Self::Forbidden {
            reason: reason.into(),
        }
    }
}

/// Sandbox preference for a tool
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxPreference {
    /// Let the system decide based on policy
    #[default]
    Auto,
    /// Require sandboxing
    Require,
    /// Forbid sandboxing (e.g., for tools that need full access)
    Forbid,
}

/// Result of tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool_use_id: String,
    pub output: ToolOutput,
    #[serde(default)]
    pub is_error: bool,
}

impl ToolResult {
    pub fn success(tool_use_id: impl Into<String>, output: ToolOutput) -> Self {
        Self {
            tool_use_id: tool_use_id.into(),
            output,
            is_error: false,
        }
    }

    pub fn error(tool_use_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            tool_use_id: tool_use_id.into(),
            output: ToolOutput::text(message),
            is_error: true,
        }
    }
}

/// Output from a tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    pub content: Vec<ToolOutputContent>,
}

impl ToolOutput {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: vec![ToolOutputContent::Text { text: text.into() }],
        }
    }

    pub fn json(value: Value) -> Self {
        Self {
            content: vec![ToolOutputContent::Text {
                text: serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string()),
            }],
        }
    }

    pub fn image(media_type: impl Into<String>, data: impl Into<String>) -> Self {
        Self {
            content: vec![ToolOutputContent::Image {
                source: crate::ImageSource::Base64 {
                    media_type: media_type.into(),
                    data: data.into(),
                },
            }],
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        self.content.first().and_then(|c| {
            if let ToolOutputContent::Text { text } = c {
                Some(text.as_str())
            } else {
                None
            }
        })
    }

    pub fn as_json(&self) -> Option<Value> {
        self.as_text()
            .and_then(|text| serde_json::from_str(text).ok())
    }
}

/// Content type for tool output
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolOutputContent {
    Text { text: String },
    Image { source: crate::ImageSource },
}

/// Approval request sent to the user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub id: String,
    pub tool_name: String,
    pub tool_input: Value,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_action: Option<SuggestedAction>,
}

/// Suggested action for approval request
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestedAction {
    Approve,
    Deny,
    ApproveOnce,
    ApproveAll,
}

/// User's decision on an approval request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum ReviewDecision {
    Approve,
    Deny { reason: Option<String> },
    ApproveOnce,
    ApproveAll,
    Edit { new_input: Value },
}

impl ReviewDecision {
    pub fn is_approved(&self) -> bool {
        matches!(
            self,
            Self::Approve | Self::ApproveOnce | Self::ApproveAll | Self::Edit { .. }
        )
    }

    pub fn is_denied(&self) -> bool {
        matches!(self, Self::Deny { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_schema_builder() {
        let schema = JsonSchema::object()
            .description("A test schema")
            .with_properties(serde_json::json!({
                "name": {"type": "string"}
            }))
            .with_required(vec!["name".to_string()]);

        assert_eq!(schema.schema_type, "object");
        assert!(schema.description.is_some());
    }

    #[test]
    fn test_tool_spec() {
        let spec = ToolSpec::new("read_file", "Read a file from disk", JsonSchema::object());
        assert_eq!(spec.name, "read_file");
    }

    #[test]
    fn test_approval_requirement() {
        let skip = ApprovalRequirement::skip();
        assert!(matches!(
            skip,
            ApprovalRequirement::Skip {
                bypass_sandbox: false
            }
        ));

        let needs = ApprovalRequirement::needs_approval("Writes to disk");
        assert!(matches!(needs, ApprovalRequirement::NeedsApproval { .. }));
    }

    #[test]
    fn test_tool_result() {
        let result = ToolResult::success("tc_123", ToolOutput::text("Done!"));
        assert!(!result.is_error);
        assert_eq!(result.output.as_text(), Some("Done!"));

        let error = ToolResult::error("tc_456", "File not found");
        assert!(error.is_error);
    }

    #[test]
    fn test_review_decision() {
        assert!(ReviewDecision::Approve.is_approved());
        assert!(ReviewDecision::ApproveOnce.is_approved());
        assert!(!ReviewDecision::Deny { reason: None }.is_approved());
        assert!(ReviewDecision::Deny { reason: None }.is_denied());
    }
}
