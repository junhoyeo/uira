use serde::{Deserialize, Serialize};

/// Complexity tier (imported from model_routing)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ComplexityTier {
    Low,
    Medium,
    High,
}

/// Semantic categories for delegation that map to complexity tiers + configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DelegationCategory {
    VisualEngineering,
    Ultrabrain,
    Artistry,
    Quick,
    Writing,
    UnspecifiedLow,
    UnspecifiedHigh,
}

impl DelegationCategory {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "visual-engineering" | "visualengineering" => Some(Self::VisualEngineering),
            "ultrabrain" => Some(Self::Ultrabrain),
            "artistry" => Some(Self::Artistry),
            "quick" => Some(Self::Quick),
            "writing" => Some(Self::Writing),
            "unspecified-low" | "unspecifiedlow" => Some(Self::UnspecifiedLow),
            "unspecified-high" | "unspecifiedhigh" => Some(Self::UnspecifiedHigh),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::VisualEngineering => "visual-engineering",
            Self::Ultrabrain => "ultrabrain",
            Self::Artistry => "artistry",
            Self::Quick => "quick",
            Self::Writing => "writing",
            Self::UnspecifiedLow => "unspecified-low",
            Self::UnspecifiedHigh => "unspecified-high",
        }
    }
}

/// Thinking budget levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ThinkingBudget {
    Low,
    Medium,
    High,
    Max,
}

/// Configuration for a delegation category
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryConfig {
    /// Complexity tier (LOW/MEDIUM/HIGH)
    pub tier: ComplexityTier,
    /// Temperature for model sampling (0-1)
    pub temperature: f64,
    /// Thinking budget level
    pub thinking_budget: ThinkingBudget,
    /// Optional prompt appendix for this category
    pub prompt_append: Option<String>,
    /// Human-readable description
    pub description: String,
}

/// Resolved category with full configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedCategory {
    /// The category identifier
    pub category: DelegationCategory,
    /// Complexity tier
    pub tier: ComplexityTier,
    /// Temperature
    pub temperature: f64,
    /// Thinking budget
    pub thinking_budget: ThinkingBudget,
    /// Description
    pub description: String,
    /// Prompt appendix
    pub prompt_append: Option<String>,
}

/// Context for category resolution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryContext {
    /// Task description
    pub task_prompt: String,
    /// Agent type being delegated to
    pub agent_type: Option<String>,
    /// Explicitly specified category (overrides detection)
    pub explicit_category: Option<DelegationCategory>,
    /// Explicitly specified tier (bypasses categories)
    pub explicit_tier: Option<ComplexityTier>,
}
