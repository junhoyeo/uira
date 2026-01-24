use crate::types::{ToolDefinition, ToolError, ToolInput, ToolOutput};
use serde_json::json;
use std::sync::Arc;

pub fn tool_definition() -> ToolDefinition {
    ToolDefinition::new(
        "skill",
        "Load a skill and return its instructions with optional argument substitution.",
        json!({
            "type": "object",
            "properties": {
                "skill": {
                    "type": "string",
                    "description": "Skill name, e.g., 'commit', 'review-pr', 'oh-my-claudecode:autopilot'"
                },
                "args": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Arguments to pass to the skill (optional)"
                }
            },
            "required": ["skill"]
        }),
        Arc::new(|input: ToolInput| {
            Box::pin(async move { handle_skill(input).await })
        }),
    )
}

async fn handle_skill(input: ToolInput) -> Result<ToolOutput, ToolError> {
    // Parse parameters
    let skill_name = input
        .get("skill")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::InvalidInput {
            message: "Missing required parameter 'skill'".to_string(),
        })?;

    let args: Vec<String> = input
        .get("args")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    // Load skill - try builtin first, handling both namespaced and non-namespaced names
    let skill = astrape_features::builtin_skills::get_builtin_skill(skill_name)
        .or_else(|| {
            // If skill_name has a namespace (e.g., "oh-my-claudecode:autopilot"),
            // try looking up just the name part after the colon
            if let Some(idx) = skill_name.rfind(':') {
                let name_without_namespace = &skill_name[idx + 1..];
                astrape_features::builtin_skills::get_builtin_skill(name_without_namespace)
            } else {
                None
            }
        })
        .ok_or_else(|| ToolError::NotFound {
            name: format!("Skill '{}' not found", skill_name),
        })?;

    // Process argument substitution if args were provided
    let (template, args_applied) = if args.is_empty() {
        (skill.template.clone(), false)
    } else {
        (apply_arguments(&skill.template, &args), true)
    };

    // Build response
    let response = json!({
        "skill": skill.name,
        "description": skill.description,
        "template": template,
        "args_applied": args_applied,
        "agent": skill.agent,
        "model": skill.model,
        "allowed_tools": skill.allowed_tools,
        "argument_hint": skill.argument_hint,
    });

    Ok(ToolOutput::text(
        serde_json::to_string_pretty(&response).unwrap_or_else(|_| response.to_string()),
    ))
}

/// Apply argument substitution to template
/// Replaces $1, $2, ... with args[0], args[1], ...
/// Replaces $@ with all args joined by space
fn apply_arguments(template: &str, args: &[String]) -> String {
    let mut result = template.to_string();

    // Replace numbered placeholders ($1, $2, etc.)
    for (i, arg) in args.iter().enumerate() {
        let placeholder = format!("${}", i + 1);
        result = result.replace(&placeholder, arg);
    }

    // Replace $@ with all args joined
    if !args.is_empty() {
        let all_args = args.join(" ");
        result = result.replace("$@", &all_args);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_arguments() {
        let template = "Commit message: $1\nDescription: $2\nAll args: $@";
        let args = vec!["feat: add feature".to_string(), "Added cool feature".to_string()];

        let result = apply_arguments(template, &args);
        assert_eq!(
            result,
            "Commit message: feat: add feature\nDescription: Added cool feature\nAll args: feat: add feature Added cool feature"
        );
    }

    #[test]
    fn test_apply_arguments_empty() {
        let template = "No args here";
        let args = vec![];

        let result = apply_arguments(template, &args);
        assert_eq!(result, "No args here");
    }
}
