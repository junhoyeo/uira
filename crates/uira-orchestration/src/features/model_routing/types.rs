use std::collections::HashMap;

use crate::sdk::ModelType;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "UPPERCASE")]
pub enum ModelTier {
    Low,
    Medium,
    High,
}

impl ModelTier {
    pub fn as_str(&self) -> &'static str {
        match self {
            ModelTier::Low => "LOW",
            ModelTier::Medium => "MEDIUM",
            ModelTier::High => "HIGH",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestionDepth {
    Why,
    How,
    What,
    Where,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainSpecificity {
    Generic,
    Frontend,
    Backend,
    Infrastructure,
    Security,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reversibility {
    Easy,
    Moderate,
    Difficult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImpactScope {
    Local,
    Module,
    SystemWide,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexicalSignals {
    pub word_count: usize,
    pub file_path_count: usize,
    pub code_block_count: usize,
    pub has_architecture_keywords: bool,
    pub has_debugging_keywords: bool,
    pub has_simple_keywords: bool,
    pub has_risk_keywords: bool,
    pub question_depth: QuestionDepth,
    pub has_implicit_requirements: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuralSignals {
    pub estimated_subtasks: usize,
    pub cross_file_dependencies: bool,
    pub has_test_requirements: bool,
    pub domain_specificity: DomainSpecificity,
    pub requires_external_knowledge: bool,
    pub reversibility: Reversibility,
    pub impact_scope: ImpactScope,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextSignals {
    pub previous_failures: u32,
    pub conversation_turns: u32,
    pub plan_complexity: u32,
    pub remaining_tasks: u32,
    pub agent_chain_depth: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComplexitySignal {
    pub lexical: LexicalSignals,
    pub structural: StructuralSignals,
    pub context: ContextSignals,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutingDecision {
    pub model: String,
    pub model_type: ModelType,
    pub tier: ModelTier,
    pub confidence: f64,
    pub reasons: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapted_prompt: Option<String>,
    pub escalated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_tier: Option<ModelTier>,
}

#[derive(Debug, Clone, Default)]
pub struct RoutingContext {
    pub task_prompt: String,
    pub agent_type: Option<String>,
    pub parent_session: Option<String>,
    pub previous_failures: Option<u32>,
    pub conversation_turns: Option<u32>,
    pub plan_tasks: Option<u32>,
    pub remaining_tasks: Option<u32>,
    pub agent_chain_depth: Option<u32>,
    pub explicit_model: Option<ModelType>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentOverride {
    pub tier: ModelTier,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TierModels {
    pub low: String,
    pub medium: String,
    pub high: String,
}

impl TierModels {
    pub fn for_tier(&self, tier: ModelTier) -> &str {
        match tier {
            ModelTier::Low => &self.low,
            ModelTier::Medium => &self.medium,
            ModelTier::High => &self.high,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RoutingConfig {
    pub enabled: bool,
    pub default_tier: ModelTier,
    pub escalation_enabled: bool,
    pub max_escalations: u32,
    pub escalation_threshold: f64,
    pub tier_models: TierModels,
    pub agent_overrides: HashMap<String, AgentOverride>,
    pub escalation_keywords: Vec<String>,
    pub simplification_keywords: Vec<String>,
}

impl Default for RoutingConfig {
    fn default() -> Self {
        let mut agent_overrides = HashMap::new();
        agent_overrides.insert(
            "coordinator".to_string(),
            AgentOverride {
                tier: ModelTier::High,
                reason: "Orchestrator requires Opus to analyze and delegate".to_string(),
            },
        );

        Self {
            enabled: true,
            default_tier: ModelTier::Medium,
            escalation_enabled: false,
            max_escalations: 0,
            escalation_threshold: 0.5,
            tier_models: TierModels {
                low: "claude-haiku-4-5-20251001".to_string(),
                medium: "claude-sonnet-4-5-20250929".to_string(),
                high: "claude-opus-4-5-20251101".to_string(),
            },
            agent_overrides,
            escalation_keywords: vec![
                "critical",
                "production",
                "urgent",
                "security",
                "breaking",
                "architecture",
                "refactor",
                "redesign",
                "root cause",
            ]
            .into_iter()
            .map(|s| s.to_string())
            .collect(),
            simplification_keywords: vec![
                "find", "list", "show", "where", "search", "locate", "grep",
            ]
            .into_iter()
            .map(|s| s.to_string())
            .collect(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct RoutingConfigOverrides {
    pub enabled: Option<bool>,
    pub default_tier: Option<ModelTier>,
    pub escalation_enabled: Option<bool>,
    pub max_escalations: Option<u32>,
    pub escalation_threshold: Option<f64>,
    pub tier_models: Option<TierModels>,
    pub agent_overrides: Option<HashMap<String, AgentOverride>>,
    pub escalation_keywords: Option<Vec<String>>,
    pub simplification_keywords: Option<Vec<String>>,
}

impl RoutingConfigOverrides {
    pub fn merge_with_default(self) -> RoutingConfig {
        let mut cfg = RoutingConfig::default();

        if let Some(v) = self.enabled {
            cfg.enabled = v;
        }
        if let Some(v) = self.default_tier {
            cfg.default_tier = v;
        }
        if let Some(v) = self.escalation_enabled {
            cfg.escalation_enabled = v;
        }
        if let Some(v) = self.max_escalations {
            cfg.max_escalations = v;
        }
        if let Some(v) = self.escalation_threshold {
            cfg.escalation_threshold = v;
        }
        if let Some(v) = self.tier_models {
            cfg.tier_models = v;
        }
        if let Some(v) = self.agent_overrides {
            cfg.agent_overrides = v;
        }
        if let Some(v) = self.escalation_keywords {
            cfg.escalation_keywords = v;
        }
        if let Some(v) = self.simplification_keywords {
            cfg.simplification_keywords = v;
        }

        cfg
    }
}

pub fn tier_to_model_type(tier: ModelTier) -> ModelType {
    match tier {
        ModelTier::Low => ModelType::Haiku,
        ModelTier::Medium => ModelType::Sonnet,
        ModelTier::High => ModelType::Opus,
    }
}

pub fn model_type_to_tier(model_type: ModelType) -> ModelTier {
    match model_type {
        ModelType::Opus => ModelTier::High,
        ModelType::Haiku => ModelTier::Low,
        ModelType::Sonnet | ModelType::Inherit => ModelTier::Medium,
    }
}

pub struct ComplexityKeywords {
    pub architecture: &'static [&'static str],
    pub debugging: &'static [&'static str],
    pub simple: &'static [&'static str],
    pub risk: &'static [&'static str],
}

pub const COMPLEXITY_KEYWORDS: ComplexityKeywords = ComplexityKeywords {
    architecture: &[
        "architecture",
        "refactor",
        "redesign",
        "restructure",
        "reorganize",
        "decouple",
        "modularize",
        "abstract",
        "pattern",
        "design",
    ],
    debugging: &[
        "debug",
        "diagnose",
        "root cause",
        "investigate",
        "trace",
        "analyze",
        "why is",
        "figure out",
        "understand why",
        "not working",
    ],
    simple: &[
        "find", "search", "locate", "list", "show", "where is", "what is", "get", "fetch",
        "display", "print",
    ],
    risk: &[
        "critical",
        "production",
        "urgent",
        "security",
        "breaking",
        "dangerous",
        "irreversible",
        "data loss",
        "migration",
        "deploy",
    ],
};
