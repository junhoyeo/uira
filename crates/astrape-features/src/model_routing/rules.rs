use crate::model_routing::types::{ComplexitySignal, ModelTier, RoutingContext};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TierSelection {
    Tier(ModelTier),
    Explicit,
}

#[derive(Debug, Clone, Copy)]
pub struct RoutingRule {
    pub name: &'static str,
    pub condition: fn(&RoutingContext, &ComplexitySignal) -> bool,
    pub action: TierSelection,
    pub reason: &'static str,
    pub priority: i32,
}

pub fn create_rule(
    name: &'static str,
    condition: fn(&RoutingContext, &ComplexitySignal) -> bool,
    tier: ModelTier,
    reason: &'static str,
    priority: i32,
) -> RoutingRule {
    RoutingRule {
        name,
        condition,
        action: TierSelection::Tier(tier),
        reason,
        priority,
    }
}

pub fn merge_rules(custom_rules: Vec<RoutingRule>) -> Vec<RoutingRule> {
    let custom_names: std::collections::HashSet<&'static str> =
        custom_rules.iter().map(|r| r.name).collect();
    let defaults: Vec<RoutingRule> = default_routing_rules()
        .into_iter()
        .filter(|r| !custom_names.contains(r.name))
        .collect();
    custom_rules.into_iter().chain(defaults).collect()
}

pub fn evaluate_rules(
    context: &RoutingContext,
    signals: &ComplexitySignal,
    rules: &[RoutingRule],
) -> (TierSelection, &'static str, &'static str) {
    let mut sorted = rules.to_vec();
    sorted.sort_by(|a, b| b.priority.cmp(&a.priority));

    for rule in sorted {
        if (rule.condition)(context, signals) {
            return (rule.action, rule.reason, rule.name);
        }
    }

    (
        TierSelection::Tier(ModelTier::Medium),
        "Fallback to medium tier",
        "fallback",
    )
}

pub fn get_matching_rules(
    context: &RoutingContext,
    signals: &ComplexitySignal,
    rules: &[RoutingRule],
) -> Vec<RoutingRule> {
    rules
        .iter()
        .copied()
        .filter(|r| (r.condition)(context, signals))
        .collect()
}

fn agent_is(ctx: &RoutingContext, name: &str) -> bool {
    ctx.agent_type.as_deref() == Some(name)
}

fn explicit_model_specified(ctx: &RoutingContext, _signals: &ComplexitySignal) -> bool {
    ctx.explicit_model.is_some()
}

fn orchestrator_fixed_opus(ctx: &RoutingContext, _signals: &ComplexitySignal) -> bool {
    agent_is(ctx, "coordinator")
}

fn architect_complex_debugging(ctx: &RoutingContext, signals: &ComplexitySignal) -> bool {
    agent_is(ctx, "architect")
        && (signals.lexical.has_debugging_keywords
            || signals.lexical.has_architecture_keywords
            || signals.lexical.has_risk_keywords)
}

fn architect_simple_lookup(ctx: &RoutingContext, signals: &ComplexitySignal) -> bool {
    agent_is(ctx, "architect")
        && signals.lexical.has_simple_keywords
        && !signals.lexical.has_debugging_keywords
        && !signals.lexical.has_architecture_keywords
        && !signals.lexical.has_risk_keywords
}

fn planner_simple_breakdown(ctx: &RoutingContext, signals: &ComplexitySignal) -> bool {
    agent_is(ctx, "planner")
        && signals.structural.estimated_subtasks <= 3
        && !signals.lexical.has_risk_keywords
        && matches!(
            signals.structural.impact_scope,
            crate::model_routing::types::ImpactScope::Local
        )
}

fn planner_strategic_planning(ctx: &RoutingContext, signals: &ComplexitySignal) -> bool {
    agent_is(ctx, "planner")
        && (matches!(
            signals.structural.impact_scope,
            crate::model_routing::types::ImpactScope::SystemWide
        ) || signals.lexical.has_architecture_keywords
            || signals.structural.estimated_subtasks > 10)
}

fn critic_checklist_review(ctx: &RoutingContext, signals: &ComplexitySignal) -> bool {
    agent_is(ctx, "critic") && signals.lexical.word_count < 30 && !signals.lexical.has_risk_keywords
}

fn critic_adversarial_review(ctx: &RoutingContext, signals: &ComplexitySignal) -> bool {
    agent_is(ctx, "critic")
        && (signals.lexical.has_risk_keywords
            || matches!(
                signals.structural.impact_scope,
                crate::model_routing::types::ImpactScope::SystemWide
            ))
}

fn analyst_simple_impact(ctx: &RoutingContext, signals: &ComplexitySignal) -> bool {
    agent_is(ctx, "analyst")
        && matches!(
            signals.structural.impact_scope,
            crate::model_routing::types::ImpactScope::Local
        )
        && !signals.lexical.has_risk_keywords
}

fn analyst_risk_analysis(ctx: &RoutingContext, signals: &ComplexitySignal) -> bool {
    agent_is(ctx, "analyst")
        && (signals.lexical.has_risk_keywords
            || matches!(
                signals.structural.impact_scope,
                crate::model_routing::types::ImpactScope::SystemWide
            ))
}

fn architecture_system_wide(_ctx: &RoutingContext, signals: &ComplexitySignal) -> bool {
    signals.lexical.has_architecture_keywords
        && matches!(
            signals.structural.impact_scope,
            crate::model_routing::types::ImpactScope::SystemWide
        )
}

fn security_domain(_ctx: &RoutingContext, signals: &ComplexitySignal) -> bool {
    matches!(
        signals.structural.domain_specificity,
        crate::model_routing::types::DomainSpecificity::Security
    )
}

fn difficult_reversibility_risk(_ctx: &RoutingContext, signals: &ComplexitySignal) -> bool {
    matches!(
        signals.structural.reversibility,
        crate::model_routing::types::Reversibility::Difficult
    ) && signals.lexical.has_risk_keywords
}

fn deep_debugging(_ctx: &RoutingContext, signals: &ComplexitySignal) -> bool {
    signals.lexical.has_debugging_keywords
        && matches!(
            signals.lexical.question_depth,
            crate::model_routing::types::QuestionDepth::Why
        )
}

fn complex_multi_step(_ctx: &RoutingContext, signals: &ComplexitySignal) -> bool {
    signals.structural.estimated_subtasks > 5 && signals.structural.cross_file_dependencies
}

fn simple_search_query(_ctx: &RoutingContext, signals: &ComplexitySignal) -> bool {
    signals.lexical.has_simple_keywords
        && signals.structural.estimated_subtasks <= 1
        && matches!(
            signals.structural.impact_scope,
            crate::model_routing::types::ImpactScope::Local
        )
        && !signals.lexical.has_architecture_keywords
        && !signals.lexical.has_debugging_keywords
}

fn short_local_change(_ctx: &RoutingContext, signals: &ComplexitySignal) -> bool {
    signals.lexical.word_count < 50
        && matches!(
            signals.structural.impact_scope,
            crate::model_routing::types::ImpactScope::Local
        )
        && matches!(
            signals.structural.reversibility,
            crate::model_routing::types::Reversibility::Easy
        )
        && !signals.lexical.has_risk_keywords
}

fn moderate_complexity(_ctx: &RoutingContext, signals: &ComplexitySignal) -> bool {
    (2..=5).contains(&signals.structural.estimated_subtasks)
}

fn module_level_work(_ctx: &RoutingContext, signals: &ComplexitySignal) -> bool {
    matches!(
        signals.structural.impact_scope,
        crate::model_routing::types::ImpactScope::Module
    )
}

fn default_medium(_ctx: &RoutingContext, _signals: &ComplexitySignal) -> bool {
    true
}

pub fn default_routing_rules() -> Vec<RoutingRule> {
    vec![
        RoutingRule {
            name: "explicit-model-specified",
            condition: explicit_model_specified,
            action: TierSelection::Explicit,
            reason: "User specified model explicitly",
            priority: 100,
        },
        RoutingRule {
            name: "orchestrator-fixed-opus",
            condition: orchestrator_fixed_opus,
            action: TierSelection::Tier(ModelTier::High),
            reason: "Orchestrator requires Opus to analyze complexity and delegate",
            priority: 90,
        },
        RoutingRule {
            name: "architect-complex-debugging",
            condition: architect_complex_debugging,
            action: TierSelection::Tier(ModelTier::High),
            reason: "Architect: Complex debugging/architecture decision",
            priority: 85,
        },
        RoutingRule {
            name: "architect-simple-lookup",
            condition: architect_simple_lookup,
            action: TierSelection::Tier(ModelTier::Low),
            reason: "Architect: Simple lookup query",
            priority: 80,
        },
        RoutingRule {
            name: "planner-simple-breakdown",
            condition: planner_simple_breakdown,
            action: TierSelection::Tier(ModelTier::Low),
            reason: "Planner: Simple task breakdown",
            priority: 75,
        },
        RoutingRule {
            name: "planner-strategic-planning",
            condition: planner_strategic_planning,
            action: TierSelection::Tier(ModelTier::High),
            reason: "Planner: Cross-domain strategic planning",
            priority: 75,
        },
        RoutingRule {
            name: "critic-checklist-review",
            condition: critic_checklist_review,
            action: TierSelection::Tier(ModelTier::Low),
            reason: "Critic: Checklist verification",
            priority: 75,
        },
        RoutingRule {
            name: "critic-adversarial-review",
            condition: critic_adversarial_review,
            action: TierSelection::Tier(ModelTier::High),
            reason: "Critic: Adversarial review for critical system",
            priority: 75,
        },
        RoutingRule {
            name: "analyst-simple-impact",
            condition: analyst_simple_impact,
            action: TierSelection::Tier(ModelTier::Low),
            reason: "Analyst: Simple impact analysis",
            priority: 75,
        },
        RoutingRule {
            name: "analyst-risk-analysis",
            condition: analyst_risk_analysis,
            action: TierSelection::Tier(ModelTier::High),
            reason: "Analyst: Risk analysis and unknown-unknowns detection",
            priority: 75,
        },
        RoutingRule {
            name: "architecture-system-wide",
            condition: architecture_system_wide,
            action: TierSelection::Tier(ModelTier::High),
            reason: "Architectural decisions with system-wide impact",
            priority: 70,
        },
        RoutingRule {
            name: "security-domain",
            condition: security_domain,
            action: TierSelection::Tier(ModelTier::High),
            reason: "Security-related tasks require careful reasoning",
            priority: 70,
        },
        RoutingRule {
            name: "difficult-reversibility-risk",
            condition: difficult_reversibility_risk,
            action: TierSelection::Tier(ModelTier::High),
            reason: "High-risk, difficult-to-reverse changes",
            priority: 70,
        },
        RoutingRule {
            name: "deep-debugging",
            condition: deep_debugging,
            action: TierSelection::Tier(ModelTier::High),
            reason: "Root cause analysis requires deep reasoning",
            priority: 65,
        },
        RoutingRule {
            name: "complex-multi-step",
            condition: complex_multi_step,
            action: TierSelection::Tier(ModelTier::High),
            reason: "Complex multi-step task with cross-file changes",
            priority: 60,
        },
        RoutingRule {
            name: "simple-search-query",
            condition: simple_search_query,
            action: TierSelection::Tier(ModelTier::Low),
            reason: "Simple search or lookup task",
            priority: 60,
        },
        RoutingRule {
            name: "short-local-change",
            condition: short_local_change,
            action: TierSelection::Tier(ModelTier::Low),
            reason: "Short, local, easily reversible change",
            priority: 55,
        },
        RoutingRule {
            name: "moderate-complexity",
            condition: moderate_complexity,
            action: TierSelection::Tier(ModelTier::Medium),
            reason: "Moderate complexity with multiple subtasks",
            priority: 50,
        },
        RoutingRule {
            name: "module-level-work",
            condition: module_level_work,
            action: TierSelection::Tier(ModelTier::Medium),
            reason: "Module-level changes",
            priority: 45,
        },
        RoutingRule {
            name: "default-medium",
            condition: default_medium,
            action: TierSelection::Tier(ModelTier::Medium),
            reason: "Default tier for unclassified tasks",
            priority: 0,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_routing::signals::extract_all_signals;

    #[test]
    fn explicit_model_rule_matches() {
        let mut ctx = RoutingContext::default();
        ctx.task_prompt = "hello".to_string();
        ctx.explicit_model = Some(astrape_sdk::ModelType::Opus);
        let signals = extract_all_signals(&ctx.task_prompt, &ctx);

        let rules = default_routing_rules();
        let (tier, _reason, name) = evaluate_rules(&ctx, &signals, &rules);
        assert_eq!(name, "explicit-model-specified");
        assert!(matches!(tier, TierSelection::Explicit));
    }
}
