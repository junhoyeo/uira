//! Planning Pipeline Coordinator
//!
//! Orchestrates the 3-stage planning flow: Analyst -> Planner -> Critic.
//! Each stage generates a prompt that incorporates the output from the previous stage.

/// Stages in the planning pipeline
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanningStage {
    /// Pre-planning analysis: intent classification, ambiguity detection, risk assessment
    Analysis,
    /// Strategic planning: detailed implementation plan with verification criteria
    Planning,
    /// Plan review: approval-biased review with blocking-only feedback
    Review,
}

impl PlanningStage {
    /// Get the next stage in the pipeline, if any
    pub fn next(&self) -> Option<Self> {
        match self {
            Self::Analysis => Some(Self::Planning),
            Self::Planning => Some(Self::Review),
            Self::Review => None,
        }
    }

    /// Get the agent type for this stage
    pub fn agent_type(&self) -> &'static str {
        match self {
            Self::Analysis => "analyst",
            Self::Planning => "planner",
            Self::Review => "critic",
        }
    }

    /// Get the system prompt for this stage
    pub fn system_prompt(&self) -> &'static str {
        match self {
            Self::Analysis => super::prompts::ANALYST_PROMPT,
            Self::Planning => super::prompts::PLANNER_PROMPT,
            Self::Review => super::prompts::CRITIC_PROMPT,
        }
    }
}

/// Represents the output from a pipeline stage
#[derive(Debug, Clone)]
pub struct StageOutput {
    pub stage: PlanningStage,
    pub content: String,
}

/// Planning pipeline that coordinates analyst -> planner -> critic flow
#[derive(Debug, Clone)]
pub struct PlanningPipeline {
    /// The original user request
    pub request: String,
    /// Accumulated stage outputs
    pub stages: Vec<StageOutput>,
}

impl PlanningPipeline {
    /// Create a new pipeline for a user request
    pub fn new(request: impl Into<String>) -> Self {
        Self {
            request: request.into(),
            stages: Vec::new(),
        }
    }

    /// Get the current stage (next stage that hasn't been completed)
    pub fn current_stage(&self) -> PlanningStage {
        match self.stages.len() {
            0 => PlanningStage::Analysis,
            1 => PlanningStage::Planning,
            _ => PlanningStage::Review,
        }
    }

    /// Check if the pipeline is complete
    pub fn is_complete(&self) -> bool {
        self.stages.len() >= 3
    }

    /// Build the prompt for the current stage, incorporating previous stage outputs
    pub fn build_stage_prompt(&self) -> String {
        let stage = self.current_stage();
        match stage {
            PlanningStage::Analysis => {
                format!(
                    "Analyze the following request and produce structured directives for the planner.\n\n\
                     <user-request>\n{}\n</user-request>",
                    self.request
                )
            }
            PlanningStage::Planning => {
                let analyst_output = self
                    .stages
                    .first()
                    .map(|s| s.content.as_str())
                    .unwrap_or("No analyst output available.");
                format!(
                    "Create a detailed implementation plan for the following request.\n\n\
                     <user-request>\n{}\n</user-request>\n\n\
                     <analyst-directives>\n{}\n</analyst-directives>\n\n\
                     Address every [MUST] directive from the analyst. Follow the structured plan format.",
                    self.request, analyst_output
                )
            }
            PlanningStage::Review => {
                let analyst_output = self
                    .stages
                    .first()
                    .map(|s| s.content.as_str())
                    .unwrap_or("");
                let planner_output = self
                    .stages
                    .get(1)
                    .map(|s| s.content.as_str())
                    .unwrap_or("No plan available.");
                let mut prompt = format!(
                    "Review the following implementation plan. Apply the [OKAY]/[REJECT] protocol.\n\n\
                     <user-request>\n{}\n</user-request>\n\n\
                     <implementation-plan>\n{}\n</implementation-plan>",
                    self.request, planner_output
                );
                if !analyst_output.is_empty() {
                    prompt.push_str(&format!(
                        "\n\n<analyst-directives>\n{}\n</analyst-directives>\n\n\
                         Verify the plan addresses all [MUST] directives from the analyst.",
                        analyst_output
                    ));
                }
                prompt
            }
        }
    }

    /// Record the output from the current stage and advance
    pub fn record_output(&mut self, content: String) {
        let stage = self.current_stage();
        self.stages.push(StageOutput { stage, content });
    }

    /// Check if the critic approved the plan (looks for [OKAY] in last stage output)
    pub fn is_approved(&self) -> Option<bool> {
        self.stages
            .iter()
            .find(|s| s.stage == PlanningStage::Review)
            .map(|s| s.content.contains("[OKAY]"))
    }

    /// Get the final plan (planner output), if available
    pub fn get_plan(&self) -> Option<&str> {
        self.stages
            .iter()
            .find(|s| s.stage == PlanningStage::Planning)
            .map(|s| s.content.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_stages() {
        let pipeline = PlanningPipeline::new("Add user authentication");
        assert_eq!(pipeline.current_stage(), PlanningStage::Analysis);
        assert!(!pipeline.is_complete());
    }

    #[test]
    fn test_stage_progression() {
        let mut pipeline = PlanningPipeline::new("Add auth");

        assert_eq!(pipeline.current_stage(), PlanningStage::Analysis);
        pipeline.record_output("Intent: Feature\nDirectives: ...".to_string());

        assert_eq!(pipeline.current_stage(), PlanningStage::Planning);
        pipeline.record_output("Plan: 1. Add auth module...".to_string());

        assert_eq!(pipeline.current_stage(), PlanningStage::Review);
        pipeline.record_output("[OKAY] Plan is approved.".to_string());

        assert!(pipeline.is_complete());
        assert_eq!(pipeline.is_approved(), Some(true));
    }

    #[test]
    fn test_analyst_prompt_contains_request() {
        let pipeline = PlanningPipeline::new("Fix the login bug");
        let prompt = pipeline.build_stage_prompt();
        assert!(prompt.contains("Fix the login bug"));
        assert!(prompt.contains("<user-request>"));
    }

    #[test]
    fn test_planner_prompt_includes_analyst_output() {
        let mut pipeline = PlanningPipeline::new("Add feature X");
        pipeline.record_output("Intent: Feature\n[MUST] Add tests".to_string());

        let prompt = pipeline.build_stage_prompt();
        assert!(prompt.contains("<analyst-directives>"));
        assert!(prompt.contains("[MUST] Add tests"));
        assert!(prompt.contains("Add feature X"));
    }

    #[test]
    fn test_critic_prompt_includes_plan_and_directives() {
        let mut pipeline = PlanningPipeline::new("Refactor auth");
        pipeline.record_output("[MUST] Maintain backwards compat".to_string());
        pipeline.record_output("Plan: 1. Extract auth module\n2. Add tests".to_string());

        let prompt = pipeline.build_stage_prompt();
        assert!(prompt.contains("<implementation-plan>"));
        assert!(prompt.contains("Extract auth module"));
        assert!(prompt.contains("<analyst-directives>"));
        assert!(prompt.contains("[MUST] Maintain backwards compat"));
    }

    #[test]
    fn test_rejected_plan() {
        let mut pipeline = PlanningPipeline::new("Bad plan");
        pipeline.record_output("Analysis done".to_string());
        pipeline.record_output("Plan done".to_string());
        pipeline.record_output("[REJECT]\n1. BLOCKER: Missing tests".to_string());

        assert_eq!(pipeline.is_approved(), Some(false));
    }

    #[test]
    fn test_stage_agent_types() {
        assert_eq!(PlanningStage::Analysis.agent_type(), "analyst");
        assert_eq!(PlanningStage::Planning.agent_type(), "planner");
        assert_eq!(PlanningStage::Review.agent_type(), "critic");
    }

    #[test]
    fn test_stage_next() {
        assert_eq!(
            PlanningStage::Analysis.next(),
            Some(PlanningStage::Planning)
        );
        assert_eq!(PlanningStage::Planning.next(), Some(PlanningStage::Review));
        assert_eq!(PlanningStage::Review.next(), None);
    }

    #[test]
    fn test_get_plan() {
        let mut pipeline = PlanningPipeline::new("Build feature");
        assert!(pipeline.get_plan().is_none());

        pipeline.record_output("Analysis".to_string());
        assert!(pipeline.get_plan().is_none());

        pipeline.record_output("The actual plan content".to_string());
        assert_eq!(pipeline.get_plan(), Some("The actual plan content"));
    }
}
