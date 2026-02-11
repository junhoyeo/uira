pub mod types;

pub use types::*;

use std::collections::HashMap;

/// Category configuration definitions
pub fn category_configs() -> HashMap<DelegationCategory, CategoryConfig> {
    let mut configs = HashMap::new();

    configs.insert(
        DelegationCategory::VisualEngineering,
        CategoryConfig {
            tier: ComplexityTier::High,
            temperature: 0.7,
            thinking_budget: ThinkingBudget::High,
            description: "UI/visual reasoning, frontend work, design systems".to_string(),
            prompt_append: Some(
                "Focus on visual design, user experience, and aesthetic quality. Consider accessibility, responsive design, and visual hierarchy.".to_string(),
            ),
        },
    );

    configs.insert(
        DelegationCategory::Ultrabrain,
        CategoryConfig {
            tier: ComplexityTier::High,
            temperature: 0.3,
            thinking_budget: ThinkingBudget::Max,
            description: "Complex reasoning, architecture decisions, deep debugging".to_string(),
            prompt_append: Some(
                "Think deeply and systematically. Consider all edge cases, implications, and long-term consequences. Reason through the problem step by step.".to_string(),
            ),
        },
    );

    configs.insert(
        DelegationCategory::Artistry,
        CategoryConfig {
            tier: ComplexityTier::Medium,
            temperature: 0.9,
            thinking_budget: ThinkingBudget::Medium,
            description: "Creative writing, novel approaches, innovative solutions".to_string(),
            prompt_append: Some(
                "Be creative and explore unconventional solutions. Think outside the box while maintaining practical feasibility.".to_string(),
            ),
        },
    );

    configs.insert(
        DelegationCategory::Quick,
        CategoryConfig {
            tier: ComplexityTier::Low,
            temperature: 0.1,
            thinking_budget: ThinkingBudget::Low,
            description: "Simple lookups, straightforward tasks, basic operations".to_string(),
            prompt_append: Some(
                "Be concise and efficient. Focus on accuracy and speed.".to_string(),
            ),
        },
    );

    configs.insert(
        DelegationCategory::Writing,
        CategoryConfig {
            tier: ComplexityTier::Medium,
            temperature: 0.5,
            thinking_budget: ThinkingBudget::Medium,
            description: "Documentation, technical writing, content creation".to_string(),
            prompt_append: Some(
                "Focus on clarity, completeness, and proper structure. Use appropriate technical terminology while remaining accessible.".to_string(),
            ),
        },
    );

    configs.insert(
        DelegationCategory::UnspecifiedLow,
        CategoryConfig {
            tier: ComplexityTier::Low,
            temperature: 0.3,
            thinking_budget: ThinkingBudget::Low,
            description: "Default for simple tasks when category is not specified".to_string(),
            prompt_append: None,
        },
    );

    configs.insert(
        DelegationCategory::UnspecifiedMedium,
        CategoryConfig {
            tier: ComplexityTier::Medium,
            temperature: 0.5,
            thinking_budget: ThinkingBudget::Medium,
            description: "Default for moderate tasks when category is not specified".to_string(),
            prompt_append: None,
        },
    );

    configs.insert(
        DelegationCategory::UnspecifiedHigh,
        CategoryConfig {
            tier: ComplexityTier::High,
            temperature: 0.5,
            thinking_budget: ThinkingBudget::High,
            description: "Default for complex tasks when category is not specified".to_string(),
            prompt_append: None,
        },
    );

    configs
}

/// Thinking budget token limits (approximate)
pub fn thinking_budget_tokens(budget: ThinkingBudget) -> u32 {
    match budget {
        ThinkingBudget::Low => 1000,
        ThinkingBudget::Medium => 5000,
        ThinkingBudget::High => 10000,
        ThinkingBudget::Max => 32000,
    }
}

/// Keywords for category detection
fn category_keywords() -> HashMap<DelegationCategory, Vec<&'static str>> {
    let mut keywords = HashMap::new();

    keywords.insert(
        DelegationCategory::VisualEngineering,
        vec![
            "ui",
            "ux",
            "design",
            "frontend",
            "component",
            "style",
            "css",
            "visual",
            "layout",
            "responsive",
            "interface",
            "dashboard",
            "form",
            "button",
            "theme",
            "color",
            "typography",
            "animation",
            "interactive",
        ],
    );

    keywords.insert(
        DelegationCategory::Ultrabrain,
        vec![
            "architecture",
            "design pattern",
            "refactor",
            "optimize",
            "debug",
            "root cause",
            "analyze",
            "investigate",
            "complex",
            "system",
            "performance",
            "scalability",
            "concurrency",
            "race condition",
        ],
    );

    keywords.insert(
        DelegationCategory::Artistry,
        vec![
            "creative",
            "innovative",
            "novel",
            "unique",
            "original",
            "brainstorm",
            "ideate",
            "explore",
            "imagine",
            "unconventional",
        ],
    );

    keywords.insert(
        DelegationCategory::Quick,
        vec![
            "find", "search", "locate", "list", "show", "get", "fetch", "where is", "what is",
            "display", "print", "lookup",
        ],
    );

    keywords.insert(
        DelegationCategory::Writing,
        vec![
            "document", "readme", "comment", "explain", "describe", "write", "draft", "article",
            "guide", "tutorial", "docs",
        ],
    );

    keywords.insert(DelegationCategory::UnspecifiedLow, vec![]);
    keywords.insert(DelegationCategory::UnspecifiedMedium, vec![]);
    keywords.insert(DelegationCategory::UnspecifiedHigh, vec![]);

    keywords
}

/// Resolve a category to its full configuration
pub fn resolve_category(category: DelegationCategory) -> ResolvedCategory {
    let configs = category_configs();
    let config = configs.get(&category).expect("Unknown delegation category");

    ResolvedCategory {
        category,
        tier: config.tier,
        temperature: config.temperature,
        thinking_budget: config.thinking_budget,
        description: config.description.clone(),
        prompt_append: config.prompt_append.clone(),
    }
}

/// Check if a string is a valid delegation category
pub fn is_valid_category(category: &str) -> bool {
    DelegationCategory::parse(category).is_some()
}

/// Get all available categories
pub fn get_all_categories() -> Vec<DelegationCategory> {
    vec![
        DelegationCategory::VisualEngineering,
        DelegationCategory::Ultrabrain,
        DelegationCategory::Artistry,
        DelegationCategory::Quick,
        DelegationCategory::Writing,
        DelegationCategory::UnspecifiedLow,
        DelegationCategory::UnspecifiedMedium,
        DelegationCategory::UnspecifiedHigh,
    ]
}

/// Get description for a category
pub fn get_category_description(category: DelegationCategory) -> String {
    let configs = category_configs();
    configs
        .get(&category)
        .map(|c| c.description.clone())
        .unwrap_or_default()
}

/// Detect category from task prompt using keyword matching
pub fn detect_category_from_prompt(task_prompt: &str) -> Option<DelegationCategory> {
    let lower_prompt = task_prompt.to_lowercase();
    let keywords = category_keywords();
    let mut scores: HashMap<DelegationCategory, usize> = HashMap::new();

    // Initialize scores
    for category in get_all_categories() {
        scores.insert(category, 0);
    }

    // Score each category based on keyword matches
    for (category, kws) in &keywords {
        for keyword in kws {
            if lower_prompt.contains(keyword) {
                *scores.entry(*category).or_insert(0) += 1;
            }
        }
    }

    // Find highest scoring category (excluding unspecified)
    let mut max_score = 0;
    let mut best_category: Option<DelegationCategory> = None;

    for category in get_all_categories() {
        if matches!(
            category,
            DelegationCategory::UnspecifiedLow
                | DelegationCategory::UnspecifiedMedium
                | DelegationCategory::UnspecifiedHigh
        ) {
            continue;
        }

        let score = scores.get(&category).copied().unwrap_or(0);
        if score > max_score {
            max_score = score;
            best_category = Some(category);
        }
    }

    // Require at least 2 keyword matches for confidence
    if max_score >= 2 {
        best_category
    } else {
        None
    }
}

/// Get category for a task with context
pub fn get_category_for_task(context: &CategoryContext) -> ResolvedCategory {
    // Explicit tier bypasses categories
    if let Some(tier) = context.explicit_tier {
        let category = match tier {
            ComplexityTier::Low => DelegationCategory::UnspecifiedLow,
            ComplexityTier::Medium => DelegationCategory::UnspecifiedMedium,
            ComplexityTier::High => DelegationCategory::UnspecifiedHigh,
        };
        return resolve_category(category);
    }

    // Explicit category
    if let Some(category) = context.explicit_category {
        return resolve_category(category);
    }

    // Auto-detect from task prompt
    if let Some(detected) = detect_category_from_prompt(&context.task_prompt) {
        return resolve_category(detected);
    }

    // Default to medium tier
    resolve_category(DelegationCategory::UnspecifiedHigh)
}

pub fn get_category_tier(category: DelegationCategory) -> ComplexityTier {
    let configs = category_configs();
    configs
        .get(&category)
        .map(|c| c.tier)
        .unwrap_or(ComplexityTier::Medium)
}

/// Get temperature from category
pub fn get_category_temperature(category: DelegationCategory) -> f64 {
    let configs = category_configs();
    configs.get(&category).map(|c| c.temperature).unwrap_or(0.5)
}

/// Get thinking budget from category
pub fn get_category_thinking_budget(category: DelegationCategory) -> ThinkingBudget {
    let configs = category_configs();
    configs
        .get(&category)
        .map(|c| c.thinking_budget)
        .unwrap_or(ThinkingBudget::Medium)
}

/// Get thinking budget in tokens
pub fn get_category_thinking_budget_tokens(category: DelegationCategory) -> u32 {
    let budget = get_category_thinking_budget(category);
    thinking_budget_tokens(budget)
}

/// Get prompt appendix for category
pub fn get_category_prompt_append(category: DelegationCategory) -> String {
    let configs = category_configs();
    configs
        .get(&category)
        .and_then(|c| c.prompt_append.clone())
        .unwrap_or_default()
}

/// Create a delegation prompt with category-specific guidance
pub fn enhance_prompt_with_category(task_prompt: &str, category: DelegationCategory) -> String {
    let configs = category_configs();
    let config = configs.get(&category);

    if let Some(cfg) = config {
        if let Some(append) = &cfg.prompt_append {
            return format!("{}\n\n{}", task_prompt, append);
        }
    }

    task_prompt.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_category() {
        let resolved = resolve_category(DelegationCategory::Ultrabrain);
        assert_eq!(resolved.tier, ComplexityTier::High);
        assert_eq!(resolved.temperature, 0.3);
        assert_eq!(resolved.thinking_budget, ThinkingBudget::Max);
    }

    #[test]
    fn test_detect_category_from_prompt() {
        let prompt = "Design a beautiful dashboard with responsive layout";
        let detected = detect_category_from_prompt(prompt);
        assert_eq!(detected, Some(DelegationCategory::VisualEngineering));

        let prompt2 = "Debug this complex race condition in the system";
        let detected2 = detect_category_from_prompt(prompt2);
        assert_eq!(detected2, Some(DelegationCategory::Ultrabrain));
    }

    #[test]
    fn test_get_category_for_task() {
        let context = CategoryContext {
            task_prompt: "Design a beautiful UI component with animations".to_string(),
            agent_type: None,
            explicit_category: None,
            explicit_tier: None,
        };

        let resolved = get_category_for_task(&context);
        assert_eq!(resolved.category, DelegationCategory::VisualEngineering);
    }

    #[test]
    fn test_thinking_budget_tokens() {
        assert_eq!(thinking_budget_tokens(ThinkingBudget::Low), 1000);
        assert_eq!(thinking_budget_tokens(ThinkingBudget::Medium), 5000);
        assert_eq!(thinking_budget_tokens(ThinkingBudget::High), 10000);
        assert_eq!(thinking_budget_tokens(ThinkingBudget::Max), 32000);
    }

    #[test]
    fn test_enhance_prompt_with_category() {
        let base = "Create a login form";
        let enhanced = enhance_prompt_with_category(base, DelegationCategory::VisualEngineering);
        assert!(enhanced.contains(base));
        assert!(enhanced.len() > base.len());
    }
}
