pub mod prompts;
pub mod router;
pub mod rules;
pub mod scorer;
pub mod signals;
pub mod types;

pub use prompts::gpt;
pub use prompts::{
    adapt_prompt_for_model, adapt_prompt_for_tier, create_delegation_prompt,
    create_delegation_prompt_for_model, get_prompt_prefix, get_prompt_suffix,
    get_task_instructions, get_task_instructions_for_model, DelegationContext,
};
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
