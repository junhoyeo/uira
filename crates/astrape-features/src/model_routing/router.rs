use crate::model_routing::rules::{default_routing_rules, evaluate_rules, TierSelection};
use crate::model_routing::scorer::{
    calculate_complexity_score, calculate_confidence, score_to_tier,
};
use crate::model_routing::signals::extract_all_signals;
use crate::model_routing::types::{
    model_type_to_tier, tier_to_model_type, ModelTier, RoutingConfigOverrides, RoutingContext,
    RoutingDecision,
};

pub fn route_task(context: RoutingContext, config: RoutingConfigOverrides) -> RoutingDecision {
    let merged = config.merge_with_default();

    if !merged.enabled {
        return create_decision(
            merged.default_tier,
            vec!["Routing disabled, using default tier".to_string()],
            false,
            None,
            &merged,
        );
    }

    if let Some(explicit) = context.explicit_model {
        let tier = model_type_to_tier(explicit);
        return create_decision(
            tier,
            vec!["Explicit model specified by user".to_string()],
            false,
            None,
            &merged,
        );
    }

    if let Some(agent) = context.agent_type.as_deref() {
        if let Some(override_cfg) = merged.agent_overrides.get(agent) {
            return create_decision(
                override_cfg.tier,
                vec![override_cfg.reason.clone()],
                false,
                None,
                &merged,
            );
        }
    }

    let signals = extract_all_signals(&context.task_prompt, &context);
    let rules = default_routing_rules();
    let (tier_sel, reason, rule_name) = evaluate_rules(&context, &signals, &rules);

    let tier = match tier_sel {
        TierSelection::Explicit => {
            // This branch should never be reached because explicit_model is checked
            // and returned early at the top of this function.
            unreachable!(
                "TierSelection::Explicit should be handled by early return for explicit_model"
            )
        }
        TierSelection::Tier(t) => t,
    };

    let score = calculate_complexity_score(&signals);
    let score_tier = score_to_tier(score);
    let confidence = calculate_confidence(score, tier);

    let reasons = vec![
        reason.to_string(),
        format!("Rule: {rule_name}"),
        format!("Score: {score} ({} tier by score)", score_tier.as_str()),
    ];

    RoutingDecision {
        model: merged.tier_models.for_tier(tier).to_string(),
        model_type: tier_to_model_type(tier),
        tier,
        confidence,
        reasons,
        adapted_prompt: None,
        escalated: false,
        original_tier: None,
    }
}

fn create_decision(
    tier: ModelTier,
    reasons: Vec<String>,
    escalated: bool,
    original_tier: Option<ModelTier>,
    cfg: &crate::model_routing::types::RoutingConfig,
) -> RoutingDecision {
    RoutingDecision {
        model: cfg.tier_models.for_tier(tier).to_string(),
        model_type: tier_to_model_type(tier),
        tier,
        confidence: if escalated { 0.9 } else { 0.7 },
        reasons,
        adapted_prompt: None,
        escalated,
        original_tier,
    }
}

pub fn escalate_model(current: ModelTier) -> ModelTier {
    match current {
        ModelTier::Low => ModelTier::Medium,
        ModelTier::Medium => ModelTier::High,
        ModelTier::High => ModelTier::High,
    }
}

pub fn can_escalate(current: ModelTier) -> bool {
    current != ModelTier::High
}

pub fn get_routing_recommendation(
    context: RoutingContext,
    config: RoutingConfigOverrides,
) -> RoutingDecision {
    route_task(context, config)
}

pub fn route_with_escalation(
    context: RoutingContext,
    config: RoutingConfigOverrides,
) -> RoutingDecision {
    // Merge config first since merge_with_default takes ownership
    let merged = config.clone().merge_with_default();
    let mut decision = route_task(context, config);
    // Only escalate if escalation is enabled (defaults to false)
    if merged.escalation_enabled
        && decision.confidence < merged.escalation_threshold
        && can_escalate(decision.tier)
    {
        let original_tier = decision.tier;
        let new_tier = escalate_model(original_tier);
        decision.tier = new_tier;
        decision.model = merged.tier_models.for_tier(new_tier).to_string();
        decision.model_type = tier_to_model_type(new_tier);
        decision.escalated = true;
        decision.original_tier = Some(original_tier);
        decision.reasons.push(format!(
            "Escalated from {} to {} (confidence {:.2} < threshold {:.2})",
            original_tier.as_str(),
            new_tier.as_str(),
            decision.confidence,
            merged.escalation_threshold
        ));
    }

    decision
}

pub fn explain_routing(context: RoutingContext, config: RoutingConfigOverrides) -> String {
    let decision = route_task(context.clone(), config);
    let signals = extract_all_signals(&context.task_prompt, &context);

    let mut lines = vec![
        "=== Model Routing Decision ===".to_string(),
        format!(
            "Task: {}{}",
            context.task_prompt.chars().take(100).collect::<String>(),
            if context.task_prompt.chars().count() > 100 {
                "..."
            } else {
                ""
            }
        ),
        format!(
            "Agent: {}",
            context
                .agent_type
                .clone()
                .unwrap_or_else(|| "unspecified".to_string())
        ),
        "".to_string(),
        "--- Signals ---".to_string(),
        format!("Word count: {}", signals.lexical.word_count),
        format!("File paths: {}", signals.lexical.file_path_count),
        format!(
            "Architecture keywords: {}",
            signals.lexical.has_architecture_keywords
        ),
        format!(
            "Debugging keywords: {}",
            signals.lexical.has_debugging_keywords
        ),
        format!("Simple keywords: {}", signals.lexical.has_simple_keywords),
        format!("Risk keywords: {}", signals.lexical.has_risk_keywords),
        format!("Question depth: {:?}", signals.lexical.question_depth),
        format!(
            "Estimated subtasks: {}",
            signals.structural.estimated_subtasks
        ),
        format!("Cross-file: {}", signals.structural.cross_file_dependencies),
        format!("Impact scope: {:?}", signals.structural.impact_scope),
        format!("Reversibility: {:?}", signals.structural.reversibility),
        format!("Previous failures: {}", signals.context.previous_failures),
        "".to_string(),
        "--- Decision ---".to_string(),
        format!("Tier: {}", decision.tier.as_str()),
        format!("Model: {}", decision.model),
        format!("Confidence: {}", decision.confidence),
        format!("Escalated: {}", decision.escalated),
        "".to_string(),
        "--- Reasons ---".to_string(),
    ];

    for r in &decision.reasons {
        lines.push(format!("  - {r}"));
    }

    lines.join("\n")
}

pub fn quick_tier_for_agent(agent_type: &str) -> Option<ModelTier> {
    let tier = match agent_type {
        "architect" | "planner" | "critic" | "analyst" => ModelTier::High,
        "explore" | "writer" => ModelTier::Low,
        "librarian" | "executor" | "designer" | "vision" => ModelTier::Medium,
        "coordinator" => ModelTier::Medium,
        _ => return None,
    };

    Some(tier)
}

pub fn is_fixed_tier_agent(agent_type: &str) -> bool {
    agent_type == "coordinator"
}

pub fn get_model_for_task(
    agent_type: &str,
    task_prompt: &str,
    config: RoutingConfigOverrides,
) -> (astrape_sdk::ModelType, ModelTier, String) {
    if is_fixed_tier_agent(agent_type) {
        return (
            astrape_sdk::ModelType::Opus,
            ModelTier::High,
            format!("{agent_type} is an orchestrator (always Opus - analyzes and delegates)"),
        );
    }

    let decision = route_task(
        RoutingContext {
            task_prompt: task_prompt.to_string(),
            agent_type: Some(agent_type.to_string()),
            ..RoutingContext::default()
        },
        config,
    );

    (
        decision.model_type,
        decision.tier,
        decision
            .reasons
            .first()
            .cloned()
            .unwrap_or_else(|| "Complexity analysis".to_string()),
    )
}

pub fn analyze_task_complexity(
    task_prompt: &str,
    agent_type: Option<&str>,
) -> (ModelTier, String, String) {
    let ctx = RoutingContext {
        task_prompt: task_prompt.to_string(),
        agent_type: agent_type.map(|s| s.to_string()),
        ..RoutingContext::default()
    };

    let signals = extract_all_signals(&ctx.task_prompt, &ctx);
    let decision = route_task(ctx.clone(), RoutingConfigOverrides::default());

    let mut analysis_lines = vec![
        format!("**Tier: {}** -> {}", decision.tier.as_str(), decision.model),
        "".to_string(),
        "**Why:**".to_string(),
    ];

    for r in &decision.reasons {
        analysis_lines.push(format!("- {r}"));
    }

    analysis_lines.push("".to_string());
    analysis_lines.push("**Signals detected:**".to_string());
    if signals.lexical.has_architecture_keywords {
        analysis_lines.push("- Architecture keywords (refactor, redesign, etc.)".to_string());
    }
    if signals.lexical.has_risk_keywords {
        analysis_lines.push("- Risk keywords (migration, production, critical)".to_string());
    }
    if signals.lexical.has_debugging_keywords {
        analysis_lines.push("- Debugging keywords (root cause, investigate)".to_string());
    }
    if signals.structural.cross_file_dependencies {
        analysis_lines.push("- Cross-file dependencies".to_string());
    }
    if matches!(
        signals.structural.impact_scope,
        crate::model_routing::types::ImpactScope::SystemWide
    ) {
        analysis_lines.push("- System-wide impact".to_string());
    }
    if matches!(
        signals.structural.reversibility,
        crate::model_routing::types::Reversibility::Difficult
    ) {
        analysis_lines.push("- Difficult to reverse".to_string());
    }

    (decision.tier, decision.model, analysis_lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_simple_search_to_low() {
        let decision = route_task(
            RoutingContext {
                task_prompt: "find where auth is implemented".to_string(),
                agent_type: Some("explore".to_string()),
                ..RoutingContext::default()
            },
            RoutingConfigOverrides::default(),
        );

        assert_eq!(decision.tier, ModelTier::Low);
        assert_eq!(decision.model_type, astrape_sdk::ModelType::Haiku);
    }

    #[test]
    fn orchestrator_is_fixed_opus() {
        let (model, tier, _reason) = get_model_for_task(
            "coordinator",
            "do something",
            RoutingConfigOverrides::default(),
        );
        assert_eq!(model, astrape_sdk::ModelType::Opus);
        assert_eq!(tier, ModelTier::High);
    }
}
