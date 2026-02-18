use crate::agents::planning_pipeline::{PlanningPipeline, PlanningStage};
use crate::tools::types::{ToolDefinition, ToolError, ToolInput, ToolOutput};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

#[derive(Debug, Clone, Deserialize)]
struct PlanningParams {
    request: String,
}

#[derive(Debug, Clone, Serialize)]
struct PlanningResponse {
    complete: bool,
    current_stage: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    analysis: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    plan: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    review: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    approved: Option<bool>,
    stage_prompts: Vec<StagePromptInfo>,
}

#[derive(Debug, Clone, Serialize)]
struct StagePromptInfo {
    stage: String,
    agent_type: String,
    system_prompt: String,
    user_prompt: String,
}

async fn handle_planning(input: ToolInput) -> Result<ToolOutput, ToolError> {
    let params: PlanningParams = serde_json::from_value(input).map_err(|e| ToolError::InvalidInput {
        message: format!("Failed to parse planning parameters: {}", e),
    })?;

    let mut stage_prompts = Vec::new();
    let mut temp_pipeline = PlanningPipeline::new(&params.request);

    let analysis_prompt = temp_pipeline.build_stage_prompt();
    stage_prompts.push(StagePromptInfo {
        stage: "analysis".to_string(),
        agent_type: PlanningStage::Analysis.agent_type().to_string(),
        system_prompt: PlanningStage::Analysis.system_prompt().to_string(),
        user_prompt: analysis_prompt,
    });

    temp_pipeline.record_output("<analyst output will be inserted here>".to_string());
    let planning_prompt = temp_pipeline.build_stage_prompt();
    stage_prompts.push(StagePromptInfo {
        stage: "planning".to_string(),
        agent_type: PlanningStage::Planning.agent_type().to_string(),
        system_prompt: PlanningStage::Planning.system_prompt().to_string(),
        user_prompt: planning_prompt,
    });

    temp_pipeline.record_output("<planner output will be inserted here>".to_string());
    let review_prompt = temp_pipeline.build_stage_prompt();
    stage_prompts.push(StagePromptInfo {
        stage: "review".to_string(),
        agent_type: PlanningStage::Review.agent_type().to_string(),
        system_prompt: PlanningStage::Review.system_prompt().to_string(),
        user_prompt: review_prompt,
    });

    let response = PlanningResponse {
        complete: false,
        current_stage: "analysis".to_string(),
        analysis: None,
        plan: None,
        review: None,
        approved: None,
        stage_prompts,
    };

    let json_response = serde_json::to_string_pretty(&response).map_err(|e| ToolError::ExecutionFailed {
        message: format!("Failed to serialize planning response: {}", e),
    })?;

    Ok(ToolOutput::text(json_response))
}

pub fn tool_definition() -> ToolDefinition {
    ToolDefinition::new(
        "planning_pipeline",
        "Initiate a structured planning pipeline (Analyst -> Planner -> Critic). Returns stage prompts for delegation to specialized agents. Use this when a task requires thorough planning before implementation.",
        json!({
            "type": "object",
            "properties": {
                "request": {
                    "type": "string",
                    "description": "The user request or task to create a plan for"
                }
            },
            "required": ["request"]
        }),
        Arc::new(|input| Box::pin(handle_planning(input))),
    )
}
