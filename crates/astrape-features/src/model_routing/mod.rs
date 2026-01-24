pub mod router;
pub mod rules;
pub mod scorer;
pub mod signals;
pub mod types;

pub use router::{
    analyze_task_complexity, can_escalate, escalate_model, explain_routing, get_model_for_task,
    get_routing_recommendation, is_fixed_tier_agent, quick_tier_for_agent, route_task,
    route_with_escalation,
};

pub use rules::{
    create_rule, default_routing_rules, evaluate_rules, get_matching_rules, merge_rules,
};
pub use scorer::{
    calculate_complexity_score, calculate_complexity_tier, calculate_confidence,
    get_score_breakdown, score_to_tier,
};
pub use signals::{
    extract_all_signals, extract_context_signals, extract_lexical_signals,
    extract_structural_signals,
};
pub use types::*;
