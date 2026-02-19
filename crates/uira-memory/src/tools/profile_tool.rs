use anyhow::Result;
use serde_json::json;
use std::sync::Arc;

use crate::profile::UserProfile;

pub fn memory_profile_tool_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": ["view", "add", "remove"],
                "description": "Action to perform on user profile. Default: view"
            },
            "fact": {
                "type": "string",
                "description": "Fact content to add (required for 'add' action)"
            },
            "category": {
                "type": "string",
                "enum": ["preference", "fact", "decision"],
                "description": "Category of the fact (for 'add' action, default: fact)"
            },
            "fact_id": {
                "type": "string",
                "description": "ID of fact to remove (required for 'remove' action)"
            }
        }
    })
}

pub async fn memory_profile_tool(
    input: serde_json::Value,
    profile: Arc<UserProfile>,
) -> Result<String> {
    let action = input
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or("view");

    match action {
        "view" => profile.format_profile(),

        "add" => {
            let fact = input
                .get("fact")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'fact' field for add action"))?;

            let category = input
                .get("category")
                .and_then(|v| v.as_str())
                .unwrap_or("fact");

            profile.add_fact("static", category, fact)?;
            Ok(format!("Added profile fact: [{category}] {fact}"))
        }

        "remove" => {
            let fact_id = input
                .get("fact_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'fact_id' field for remove action"))?;

            let removed = profile.remove_fact(fact_id)?;
            if removed {
                Ok(format!("Removed profile fact: {fact_id}"))
            } else {
                Ok(format!("No profile fact found with ID: {fact_id}"))
            }
        }

        _ => Err(anyhow::anyhow!(
            "Unknown action: {action}. Use 'view', 'add', or 'remove'."
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::MemoryStore;

    fn setup() -> Arc<UserProfile> {
        let store = Arc::new(MemoryStore::new_in_memory(64).unwrap());
        Arc::new(UserProfile::new(store))
    }

    #[tokio::test]
    async fn view_empty_profile() {
        let profile = setup();
        let input = serde_json::json!({ "action": "view" });
        let result = memory_profile_tool(input, profile).await.unwrap();
        assert!(result.contains("No user profile"));
    }

    #[tokio::test]
    async fn add_and_view() {
        let profile = setup();

        let input = serde_json::json!({
            "action": "add",
            "fact": "Prefers dark mode",
            "category": "preference"
        });
        let result = memory_profile_tool(input, profile.clone()).await.unwrap();
        assert!(result.contains("Added"));

        let input = serde_json::json!({ "action": "view" });
        let result = memory_profile_tool(input, profile).await.unwrap();
        assert!(result.contains("Prefers dark mode"));
    }

    #[tokio::test]
    async fn remove_fact() {
        let profile = setup();

        let add_input = serde_json::json!({
            "action": "add",
            "fact": "Test fact",
            "category": "fact"
        });
        memory_profile_tool(add_input, profile.clone())
            .await
            .unwrap();

        let facts = profile.get_facts(None).unwrap();
        let id = facts[0].id.clone();

        let input = serde_json::json!({ "action": "remove", "fact_id": id });
        let result = memory_profile_tool(input, profile.clone()).await.unwrap();
        assert!(result.contains("Removed"));

        let facts = profile.get_facts(None).unwrap();
        assert!(facts.is_empty());
    }

    #[tokio::test]
    async fn default_action_is_view() {
        let profile = setup();
        let input = serde_json::json!({});
        let result = memory_profile_tool(input, profile).await.unwrap();
        assert!(result.contains("No user profile"));
    }
}
