use crate::model_routing::types::{
    ComplexitySignal, ContextSignals, LexicalSignals, ModelTier, StructuralSignals,
};

const TIER_THRESHOLD_HIGH: i32 = 8;
const TIER_THRESHOLD_MEDIUM: i32 = 4;

struct Weights;

impl Weights {
    // lexical
    const WORD_COUNT_HIGH: i32 = 2;
    const WORD_COUNT_VERY_HIGH: i32 = 1;
    const FILE_PATHS_MULTIPLE: i32 = 1;
    const CODE_BLOCKS_PRESENT: i32 = 1;
    const ARCHITECTURE_KEYWORDS: i32 = 3;
    const DEBUGGING_KEYWORDS: i32 = 2;
    const SIMPLE_KEYWORDS: i32 = -2;
    const RISK_KEYWORDS: i32 = 2;
    const QUESTION_DEPTH_WHY: i32 = 2;
    const QUESTION_DEPTH_HOW: i32 = 1;
    const IMPLICIT_REQUIREMENTS: i32 = 1;

    // structural
    const SUBTASKS_MANY: i32 = 3;
    const SUBTASKS_SOME: i32 = 1;
    const CROSS_FILE: i32 = 2;
    const TEST_REQUIRED: i32 = 1;
    const SECURITY_DOMAIN: i32 = 2;
    const INFRA_DOMAIN: i32 = 1;
    const EXTERNAL_KNOWLEDGE: i32 = 1;
    const REVERSIBILITY_DIFFICULT: i32 = 2;
    const REVERSIBILITY_MODERATE: i32 = 1;
    const IMPACT_SYSTEM_WIDE: i32 = 3;
    const IMPACT_MODULE: i32 = 1;

    // context
    const PREVIOUS_FAILURE: i32 = 2;
    const PREVIOUS_FAILURE_MAX: i32 = 4;
    const DEEP_CHAIN: i32 = 2;
    const COMPLEX_PLAN: i32 = 1;
}

fn score_lexical(signals: &LexicalSignals) -> i32 {
    let mut score = 0;

    if signals.word_count > 200 {
        score += Weights::WORD_COUNT_HIGH;
        if signals.word_count > 500 {
            score += Weights::WORD_COUNT_VERY_HIGH;
        }
    }

    if signals.file_path_count >= 2 {
        score += Weights::FILE_PATHS_MULTIPLE;
    }

    if signals.code_block_count > 0 {
        score += Weights::CODE_BLOCKS_PRESENT;
    }

    if signals.has_architecture_keywords {
        score += Weights::ARCHITECTURE_KEYWORDS;
    }
    if signals.has_debugging_keywords {
        score += Weights::DEBUGGING_KEYWORDS;
    }
    if signals.has_simple_keywords {
        score += Weights::SIMPLE_KEYWORDS;
    }
    if signals.has_risk_keywords {
        score += Weights::RISK_KEYWORDS;
    }

    match signals.question_depth {
        crate::model_routing::types::QuestionDepth::Why => score += Weights::QUESTION_DEPTH_WHY,
        crate::model_routing::types::QuestionDepth::How => score += Weights::QUESTION_DEPTH_HOW,
        _ => {}
    }

    if signals.has_implicit_requirements {
        score += Weights::IMPLICIT_REQUIREMENTS;
    }

    score
}

fn score_structural(signals: &StructuralSignals) -> i32 {
    let mut score = 0;

    if signals.estimated_subtasks > 3 {
        score += Weights::SUBTASKS_MANY;
    } else if signals.estimated_subtasks > 1 {
        score += Weights::SUBTASKS_SOME;
    }

    if signals.cross_file_dependencies {
        score += Weights::CROSS_FILE;
    }

    if signals.has_test_requirements {
        score += Weights::TEST_REQUIRED;
    }

    match signals.domain_specificity {
        crate::model_routing::types::DomainSpecificity::Security => {
            score += Weights::SECURITY_DOMAIN
        }
        crate::model_routing::types::DomainSpecificity::Infrastructure => {
            score += Weights::INFRA_DOMAIN
        }
        _ => {}
    }

    if signals.requires_external_knowledge {
        score += Weights::EXTERNAL_KNOWLEDGE;
    }

    match signals.reversibility {
        crate::model_routing::types::Reversibility::Difficult => {
            score += Weights::REVERSIBILITY_DIFFICULT
        }
        crate::model_routing::types::Reversibility::Moderate => {
            score += Weights::REVERSIBILITY_MODERATE
        }
        crate::model_routing::types::Reversibility::Easy => {}
    }

    match signals.impact_scope {
        crate::model_routing::types::ImpactScope::SystemWide => {
            score += Weights::IMPACT_SYSTEM_WIDE
        }
        crate::model_routing::types::ImpactScope::Module => score += Weights::IMPACT_MODULE,
        crate::model_routing::types::ImpactScope::Local => {}
    }

    score
}

fn score_context(signals: &ContextSignals) -> i32 {
    let mut score = 0;

    let failure_score = (signals.previous_failures as i32)
        .saturating_mul(Weights::PREVIOUS_FAILURE)
        .min(Weights::PREVIOUS_FAILURE_MAX);
    score += failure_score;

    if signals.agent_chain_depth >= 3 {
        score += Weights::DEEP_CHAIN;
    }

    if signals.plan_complexity >= 5 {
        score += Weights::COMPLEX_PLAN;
    }

    score
}

pub fn calculate_complexity_score(signals: &ComplexitySignal) -> i32 {
    score_lexical(&signals.lexical)
        + score_structural(&signals.structural)
        + score_context(&signals.context)
}

pub fn score_to_tier(score: i32) -> ModelTier {
    if score >= TIER_THRESHOLD_HIGH {
        return ModelTier::High;
    }
    if score >= TIER_THRESHOLD_MEDIUM {
        return ModelTier::Medium;
    }
    ModelTier::Low
}

pub fn calculate_complexity_tier(signals: &ComplexitySignal) -> ModelTier {
    score_to_tier(calculate_complexity_score(signals))
}

pub fn get_score_breakdown(signals: &ComplexitySignal) -> (i32, i32, i32, i32, ModelTier) {
    let lexical = score_lexical(&signals.lexical);
    let structural = score_structural(&signals.structural);
    let context = score_context(&signals.context);
    let total = lexical + structural + context;
    (lexical, structural, context, total, score_to_tier(total))
}

pub fn calculate_confidence(score: i32, tier: ModelTier) -> f64 {
    let distance_from_low = (score - TIER_THRESHOLD_MEDIUM).abs();
    let distance_from_high = (score - TIER_THRESHOLD_HIGH).abs();

    let min_distance = match tier {
        ModelTier::Low => (TIER_THRESHOLD_MEDIUM - score).max(0),
        ModelTier::Medium => distance_from_low.min(distance_from_high),
        ModelTier::High => (score - TIER_THRESHOLD_HIGH).max(0),
    };

    let capped = min_distance.min(4) as f64;
    let confidence = 0.5 + (capped / 4.0) * 0.4;
    (confidence * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_routing::types::*;

    #[test]
    fn score_to_tier_matches_thresholds() {
        assert_eq!(score_to_tier(0), ModelTier::Low);
        assert_eq!(score_to_tier(4), ModelTier::Medium);
        assert_eq!(score_to_tier(8), ModelTier::High);
    }
}
