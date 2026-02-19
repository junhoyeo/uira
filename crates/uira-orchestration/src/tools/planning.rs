use crate::agents::planning_pipeline::{PlanningPipeline, PlanningStage};
use crate::tools::types::{ToolDefinition, ToolError, ToolInput, ToolOutput};
use serde::Deserialize;
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

static PLAN_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Deserialize)]
struct PlanningParams {
    request: String,
}

fn analyst_stage_prompt(request: &str, pipeline_context: &str) -> String {
    format!(
        "You are a senior technical analyst. Analyze the following request and produce a structured assessment.\n\n\
REQUEST: {request}\n\n\
PIPELINE CONTEXT:\n{pipeline_context}\n\n\
Your analysis MUST include:\n\
1. Scope: What files, modules, and systems are affected?\n\
2. Dependencies: What existing code depends on the changes?\n\
3. Risks: What could break? What are the edge cases?\n\
4. Constraints: What patterns, conventions, or limitations exist?\n\
5. Effort estimate: Small (1-2 files), Medium (3-5 files), Large (5+ files)\n\n\
Output format: Structured markdown with sections for each point above, followed by a concise checklist the planner can execute."
    )
}

fn planner_stage_prompt(request: &str, pipeline_context: &str) -> String {
    format!(
        "You are a senior technical planner. Using the analyst's assessment, create a detailed implementation plan.\n\n\
REQUEST: {request}\n\n\
ANALYST ASSESSMENT:\n{{{{analyst_output}}}}\n\n\
PIPELINE CONTEXT:\n{pipeline_context}\n\n\
Your plan MUST include:\n\
1. Steps: Numbered list of atomic implementation steps\n\
2. File changes: Specific files to create/modify with descriptions\n\
3. Test criteria: How to verify each step works\n\
4. Order: Which steps depend on others\n\
5. Rollback: How to undo if something goes wrong\n\n\
Output format: Structured markdown with numbered steps and explicit dependency markers between steps."
    )
}

fn critic_stage_prompt(request: &str, pipeline_context: &str) -> String {
    format!(
        "You are a senior technical reviewer. Review the analyst's assessment and planner's implementation plan.\n\n\
REQUEST: {request}\n\n\
ANALYST ASSESSMENT:\n{{{{analyst_output}}}}\n\n\
IMPLEMENTATION PLAN:\n{{{{planner_output}}}}\n\n\
PIPELINE CONTEXT:\n{pipeline_context}\n\n\
Your review MUST include:\n\
1. Gaps: What did the analyst miss? What did the planner overlook?\n\
2. Edge cases: What scenarios are not covered?\n\
3. Regressions: What existing functionality might break?\n\
4. Improvements: Concrete suggestions to strengthen the plan\n\
5. Confidence: Rate 1-10 with justification\n\n\
Output format: Structured markdown with sections for each point above and a final approve/revise recommendation."
    )
}

async fn handle_planning(input: ToolInput) -> Result<ToolOutput, ToolError> {
    let params: PlanningParams =
        serde_json::from_value(input).map_err(|e| ToolError::InvalidInput {
            message: format!("Failed to parse planning parameters: {}", e),
        })?;

    if params.request.trim().is_empty() {
        return Err(ToolError::InvalidInput {
            message: "Planning request cannot be empty".to_string(),
        });
    }

    let mut temp_pipeline = PlanningPipeline::new(&params.request);

    let analyst_context = temp_pipeline.build_stage_prompt();
    let analyst_prompt = analyst_stage_prompt(&params.request, &analyst_context);

    temp_pipeline.record_output("{{analyst_output}}".to_string());
    let planner_context = temp_pipeline.build_stage_prompt();
    let planner_prompt = planner_stage_prompt(&params.request, &planner_context);

    temp_pipeline.record_output("{{planner_output}}".to_string());
    let critic_context = temp_pipeline.build_stage_prompt();
    let critic_prompt = critic_stage_prompt(&params.request, &critic_context);

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| ToolError::ExecutionFailed {
            message: format!("Failed to create plan_id timestamp: {}", e),
        })?
        .as_millis();
    let seq = PLAN_COUNTER.fetch_add(1, Ordering::Relaxed);
    let plan_id = format!("{}-{}", ts, seq);

    let response = json!({
        "plan_id": plan_id,
        "request": params.request,
        "stages": [
            {
                "stage": PlanningStage::Analysis.agent_type(),
                "prompt": analyst_prompt,
                "expected_output": "Structured analysis with scope, risks, dependencies",
                "status": "ready"
            },
            {
                "stage": PlanningStage::Planning.agent_type(),
                "prompt": planner_prompt,
                "expected_output": "Step-by-step implementation plan",
                "status": "ready",
                "depends_on": [PlanningStage::Analysis.agent_type()]
            },
            {
                "stage": PlanningStage::Review.agent_type(),
                "prompt": critic_prompt,
                "expected_output": "Review with gaps, edge cases, confidence score",
                "status": "ready",
                "depends_on": [PlanningStage::Analysis.agent_type(), PlanningStage::Planning.agent_type()]
            }
        ],
        "execution_order": [
            PlanningStage::Analysis.agent_type(),
            PlanningStage::Planning.agent_type(),
            PlanningStage::Review.agent_type()
        ],
        "context_variables": {
            "{{analyst_output}}": "Replace with the analyst stage's actual output before passing to planner/critic",
            "{{planner_output}}": "Replace with the planner stage's actual output before passing to critic"
        },
        "substitution_contract": "Stage prompts contain {{analyst_output}} and {{planner_output}} template variables. Before executing each stage, replace these placeholders with the actual output from the referenced prior stage.",
        "instructions": "Execute each stage in order. Pass the output of each stage as context to the next by substituting the template variables. The final plan is the critic-reviewed planner output."
    });

    let json_response =
        serde_json::to_string_pretty(&response).map_err(|e| ToolError::ExecutionFailed {
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
