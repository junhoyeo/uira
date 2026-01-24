use lazy_static::lazy_static;
use regex::Regex;

use crate::model_routing::types::{
    ComplexitySignal, ContextSignals, DomainSpecificity, ImpactScope, LexicalSignals,
    QuestionDepth, Reversibility, RoutingContext, StructuralSignals, COMPLEXITY_KEYWORDS,
};

lazy_static! {
    // File path detection patterns
    static ref FILE_PATH_PATTERN: Regex =
        Regex::new("(?m)(?:^|\\s)[./~]?(?:[\\w-]+/)+[\\w.-]+\\.\\w+").unwrap();
    static ref FILE_IN_BACKTICKS: Regex =
        Regex::new("\\x60[^\\x60]+\\.\\w+\\x60").unwrap();
    static ref FILE_IN_QUOTES: Regex =
        Regex::new("(?:'|\\x22)[^'\\x22]+\\.\\w+(?:'|\\x22)").unwrap();

    // Question depth patterns
    static ref WHY_PATTERN: Regex =
        Regex::new(r"(?i)(\bwhy\b.*\?|\bwhy\s+(is|are|does|do|did|would|should|can))").unwrap();
    static ref HOW_PATTERN: Regex =
        Regex::new(r"(?i)(\bhow\b.*\?|\bhow\s+(do|does|can|should|would|to))").unwrap();
    static ref WHAT_PATTERN: Regex =
        Regex::new(r"(?i)(\bwhat\b.*\?|\bwhat\s+(is|are|does|do))").unwrap();
    static ref WHERE_PATTERN: Regex =
        Regex::new(r"(?i)(\bwhere\b.*\?|\bwhere\s+(is|are|does|do|can))").unwrap();

    // Code block pattern
    static ref FENCED_CODE_BLOCK: Regex =
        Regex::new(r"(?s)```.*?```").unwrap();

    // Subtask estimation patterns
    static ref BULLET_LIST: Regex =
        Regex::new(r"(?m)^[\s]*[-*]\s").unwrap();
    static ref NUMBERED_LIST: Regex =
        Regex::new(r"(?m)^[\s]*\d+[.)]\s").unwrap();
    static ref AND_WORD: Regex =
        Regex::new(r"(?i)\band\b").unwrap();
    static ref THEN_WORD: Regex =
        Regex::new(r"(?i)\bthen\b").unwrap();
}

pub fn extract_lexical_signals(prompt: &str) -> LexicalSignals {
    let lower = prompt.to_lowercase();
    let word_count = prompt.split_whitespace().filter(|w| !w.is_empty()).count();

    LexicalSignals {
        word_count,
        file_path_count: count_file_paths(prompt),
        code_block_count: count_code_blocks(prompt),
        has_architecture_keywords: has_keywords(&lower, COMPLEXITY_KEYWORDS.architecture),
        has_debugging_keywords: has_keywords(&lower, COMPLEXITY_KEYWORDS.debugging),
        has_simple_keywords: has_keywords(&lower, COMPLEXITY_KEYWORDS.simple),
        has_risk_keywords: has_keywords(&lower, COMPLEXITY_KEYWORDS.risk),
        question_depth: detect_question_depth(&lower),
        has_implicit_requirements: detect_implicit_requirements(&lower),
    }
}

pub fn extract_structural_signals(prompt: &str) -> StructuralSignals {
    let lower = prompt.to_lowercase();

    StructuralSignals {
        estimated_subtasks: estimate_subtasks(prompt),
        cross_file_dependencies: detect_cross_file_dependencies(prompt),
        has_test_requirements: detect_test_requirements(&lower),
        domain_specificity: detect_domain(&lower),
        requires_external_knowledge: detect_external_knowledge(&lower),
        reversibility: assess_reversibility(&lower),
        impact_scope: assess_impact_scope(prompt),
    }
}

pub fn extract_context_signals(context: &RoutingContext) -> ContextSignals {
    ContextSignals {
        previous_failures: context.previous_failures.unwrap_or(0),
        conversation_turns: context.conversation_turns.unwrap_or(0),
        plan_complexity: context.plan_tasks.unwrap_or(0),
        remaining_tasks: context.remaining_tasks.unwrap_or(0),
        agent_chain_depth: context.agent_chain_depth.unwrap_or(0),
    }
}

pub fn extract_all_signals(prompt: &str, context: &RoutingContext) -> ComplexitySignal {
    ComplexitySignal {
        lexical: extract_lexical_signals(prompt),
        structural: extract_structural_signals(prompt),
        context: extract_context_signals(context),
    }
}

fn count_file_paths(prompt: &str) -> usize {
    let mut count = 0usize;
    count = count.saturating_add(FILE_PATH_PATTERN.find_iter(prompt).count());
    count = count.saturating_add(FILE_IN_BACKTICKS.find_iter(prompt).count());
    count = count.saturating_add(FILE_IN_QUOTES.find_iter(prompt).count());
    count.min(20)
}

fn count_code_blocks(prompt: &str) -> usize {
    let fenced = FENCED_CODE_BLOCK.find_iter(prompt).count();

    // Approximate the indented-block heuristic: count consecutive indented lines.
    let mut indented_groups = 0usize;
    let mut in_group = false;
    for line in prompt.lines() {
        let is_indented = line.starts_with("    ") || line.starts_with('\t');
        if is_indented {
            if !in_group {
                indented_groups += 1;
                in_group = true;
            }
        } else {
            in_group = false;
        }
    }

    // TS does `Math.floor(indentedBlocks/2)`; approximate similarly.
    fenced + (indented_groups / 2)
}

fn has_keywords(prompt: &str, keywords: &[&str]) -> bool {
    keywords.iter().any(|kw| prompt.contains(kw))
}

fn detect_question_depth(prompt: &str) -> QuestionDepth {
    if WHY_PATTERN.is_match(prompt) {
        return QuestionDepth::Why;
    }
    if HOW_PATTERN.is_match(prompt) {
        return QuestionDepth::How;
    }
    if WHAT_PATTERN.is_match(prompt) {
        return QuestionDepth::What;
    }
    if WHERE_PATTERN.is_match(prompt) {
        return QuestionDepth::Where;
    }
    QuestionDepth::None
}

fn detect_implicit_requirements(prompt: &str) -> bool {
    // Rust's regex crate doesn't support lookarounds.
    // We approximate the TS behavior with conservative substring checks.
    if prompt.contains("make it better") || prompt.contains("clean up") {
        return true;
    }

    if prompt.contains("improve")
        && !prompt.contains(" by ")
        && !prompt.contains(" to ")
        && !prompt.contains("so that")
    {
        return true;
    }

    if prompt.contains("optimize")
        && !prompt.contains(" by ")
        && !prompt.contains(" for ")
        && !prompt.contains(" to ")
    {
        return true;
    }

    if prompt.contains("refactor")
        && !prompt.contains("refactor to")
        && !prompt.contains("refactor by")
        && !prompt.contains("refactor into")
    {
        return true;
    }

    if prompt.contains("fix")
        && !prompt.contains("fix the")
        && !prompt.contains("fix this")
        && !prompt.contains("fix that")
        && !prompt.contains("fix in")
        && !prompt.contains("fix at")
    {
        return true;
    }

    false
}

fn estimate_subtasks(prompt: &str) -> usize {
    let mut count = 1usize;

    count += BULLET_LIST.find_iter(prompt).count();
    count += NUMBERED_LIST.find_iter(prompt).count();

    let and_count = AND_WORD.find_iter(prompt).count();
    count += and_count / 2;

    count += THEN_WORD.find_iter(prompt).count();

    count.min(10)
}

fn detect_cross_file_dependencies(prompt: &str) -> bool {
    if count_file_paths(prompt) >= 2 {
        return true;
    }

    let indicators = [
        Regex::new(r"(?i)multiple files").unwrap(),
        Regex::new(r"(?i)across.*files").unwrap(),
        Regex::new(r"(?i)several.*files").unwrap(),
        Regex::new(r"(?i)all.*files").unwrap(),
        Regex::new(r"(?i)throughout.*codebase").unwrap(),
        Regex::new(r"(?i)entire.*project").unwrap(),
        Regex::new(r"(?i)whole.*system").unwrap(),
    ];

    indicators.iter().any(|p| p.is_match(prompt))
}

fn detect_test_requirements(prompt: &str) -> bool {
    let indicators = [
        Regex::new(r"(?i)\btest").unwrap(),
        Regex::new(r"(?i)\bspec\b").unwrap(),
        Regex::new(r"(?i)make sure.*work").unwrap(),
        Regex::new(r"(?i)verify").unwrap(),
        Regex::new(r"(?i)ensure.*pass").unwrap(),
        Regex::new(r"\bTDD\b").unwrap(),
        Regex::new(r"(?i)unit test").unwrap(),
        Regex::new(r"(?i)integration test").unwrap(),
    ];

    indicators.iter().any(|p| p.is_match(prompt))
}

fn detect_domain(prompt: &str) -> DomainSpecificity {
    let frontend = [
        Regex::new(r"(?i)\b(react|vue|angular|svelte|css|html|jsx|tsx|component|ui|ux|styling|tailwind|sass|scss)\b").unwrap(),
        Regex::new(r"(?i)\b(button|modal|form|input|layout|responsive|animation)\b").unwrap(),
    ];
    if frontend.iter().any(|p| p.is_match(prompt)) {
        return DomainSpecificity::Frontend;
    }

    let backend = [
        Regex::new(
            r"(?i)\b(api|endpoint|database|query|sql|graphql|rest|server|auth|middleware)\b",
        )
        .unwrap(),
        Regex::new(r"(?i)\b(node|express|fastify|nest|django|flask|rails)\b").unwrap(),
    ];
    if backend.iter().any(|p| p.is_match(prompt)) {
        return DomainSpecificity::Backend;
    }

    let infra = [
        Regex::new(
            r"(?i)\b(docker|kubernetes|k8s|terraform|aws|gcp|azure|ci|cd|deploy|container)\b",
        )
        .unwrap(),
        Regex::new(r"(?i)\b(nginx|load.?balancer|scaling|monitoring|logging)\b").unwrap(),
    ];
    if infra.iter().any(|p| p.is_match(prompt)) {
        return DomainSpecificity::Infrastructure;
    }

    let security = [
        Regex::new(
            r"(?i)\b(security|auth|oauth|jwt|encryption|vulnerability|xss|csrf|injection)\b",
        )
        .unwrap(),
        Regex::new(r"(?i)\b(password|credential|secret|token|permission)\b").unwrap(),
    ];
    if security.iter().any(|p| p.is_match(prompt)) {
        return DomainSpecificity::Security;
    }

    DomainSpecificity::Generic
}

fn detect_external_knowledge(prompt: &str) -> bool {
    let indicators = [
        Regex::new(r"(?i)\bdocs?\b").unwrap(),
        Regex::new(r"(?i)\bdocumentation\b").unwrap(),
        Regex::new(r"(?i)\bofficial\b").unwrap(),
        Regex::new(r"(?i)\blibrary\b").unwrap(),
        Regex::new(r"(?i)\bpackage\b").unwrap(),
        Regex::new(r"(?i)\bframework\b").unwrap(),
        Regex::new(r"(?i)how does.*work").unwrap(),
        Regex::new(r"(?i)best practice").unwrap(),
    ];

    indicators.iter().any(|p| p.is_match(prompt))
}

fn assess_reversibility(prompt: &str) -> Reversibility {
    let difficult = [
        Regex::new(r"(?i)\bmigrat").unwrap(),
        Regex::new(r"(?i)\bproduction\b").unwrap(),
        Regex::new(r"(?i)\bdata.*loss").unwrap(),
        Regex::new(r"(?i)\bdelete.*all").unwrap(),
        Regex::new(r"(?i)\bdrop.*table").unwrap(),
        Regex::new(r"(?i)\birreversible\b").unwrap(),
        Regex::new(r"(?i)\bpermanent\b").unwrap(),
    ];
    if difficult.iter().any(|p| p.is_match(prompt)) {
        return Reversibility::Difficult;
    }

    let moderate = [
        Regex::new(r"(?i)\brefactor\b").unwrap(),
        Regex::new(r"(?i)\brestructure\b").unwrap(),
        Regex::new(r"(?i)\brename.*across").unwrap(),
        Regex::new(r"(?i)\bmove.*files").unwrap(),
        Regex::new(r"(?i)\bchange.*schema").unwrap(),
    ];
    if moderate.iter().any(|p| p.is_match(prompt)) {
        return Reversibility::Moderate;
    }

    Reversibility::Easy
}

fn assess_impact_scope(prompt: &str) -> ImpactScope {
    let system = [
        Regex::new(r"(?i)\bentire\b").unwrap(),
        Regex::new(r"(?i)\ball\s+(?:files|components|modules)").unwrap(),
        Regex::new(r"(?i)\bwhole\s+(?:project|codebase|system)").unwrap(),
        Regex::new(r"(?i)\bsystem.?wide").unwrap(),
        Regex::new(r"(?i)\bglobal\b").unwrap(),
        Regex::new(r"(?i)\beverywhere\b").unwrap(),
        Regex::new(r"(?i)\bthroughout\b").unwrap(),
    ];
    if system.iter().any(|p| p.is_match(prompt)) {
        return ImpactScope::SystemWide;
    }

    let module = [
        Regex::new(r"(?i)\bmodule\b").unwrap(),
        Regex::new(r"(?i)\bpackage\b").unwrap(),
        Regex::new(r"(?i)\bservice\b").unwrap(),
        Regex::new(r"(?i)\bfeature\b").unwrap(),
        Regex::new(r"(?i)\bcomponent\b").unwrap(),
        Regex::new(r"(?i)\blayer\b").unwrap(),
    ];

    if count_file_paths(prompt) >= 3 {
        return ImpactScope::Module;
    }
    if module.iter().any(|p| p.is_match(prompt)) {
        return ImpactScope::Module;
    }

    ImpactScope::Local
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_routing::types::QuestionDepth;

    #[test]
    fn lexical_counts_files_and_depth() {
        let s = extract_lexical_signals(
            "why is auth broken in src/main.rs?\n```js\nconsole.log(1)\n```",
        );
        assert!(s.file_path_count >= 1);
        assert!(s.code_block_count >= 1);
        assert_eq!(s.question_depth, QuestionDepth::Why);
    }
}
