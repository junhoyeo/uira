use regex::Regex;
use uira_types::HookOutput;

pub struct KeywordPattern {
    pub name: &'static str,
    regex: Regex,
    message_fn: fn(Option<&str>) -> String,
}

impl KeywordPattern {
    pub fn new(name: &'static str, pattern: &str, message_fn: fn(Option<&str>) -> String) -> Self {
        Self {
            name,
            regex: Regex::new(pattern).expect("Invalid regex pattern"),
            message_fn,
        }
    }

    pub fn matches(&self, text: &str) -> bool {
        self.regex.is_match(text)
    }

    pub fn get_message(&self, agent: Option<&str>) -> String {
        (self.message_fn)(agent)
    }
}

pub struct KeywordDetector {
    patterns: Vec<KeywordPattern>,
}

impl Default for KeywordDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl KeywordDetector {
    pub fn new() -> Self {
        Self {
            patterns: vec![
                KeywordPattern::new(
                    "ralph",
                    r"(?i)\b(ralph|don't stop|must complete|until done)\b",
                    ralph_message,
                ),
                KeywordPattern::new("ultrawork", r"(?i)\b(ultrawork|ulw)\b", ultrawork_message),
                KeywordPattern::new(
                    "search",
                    r"(?i)\b(search|find|locate|lookup|explore|discover|scan|grep|query)\b|where\s+is|show\s+me|list\s+all",
                    search_message,
                ),
                KeywordPattern::new(
                    "analyze",
                    r"(?i)\b(analyze|analyse|investigate|examine|research|study|deep[\s-]?dive|inspect|audit|debug)\b|why\s+is|how\s+does|how\s+to",
                    analyze_message,
                ),
            ],
        }
    }

    pub fn with_pattern(mut self, pattern: KeywordPattern) -> Self {
        self.patterns.push(pattern);
        self
    }

    pub fn detect(&self, prompt: &str, agent: Option<&str>) -> Option<HookOutput> {
        let clean_prompt = remove_code_blocks(prompt);

        for pattern in &self.patterns {
            if pattern.matches(&clean_prompt) {
                return Some(HookOutput::with_message(&pattern.get_message(agent)));
            }
        }
        None
    }

    pub fn detect_all(&self, prompt: &str, agent: Option<&str>) -> Vec<(&'static str, String)> {
        let clean_prompt = remove_code_blocks(prompt);
        let mut results = Vec::new();

        for pattern in &self.patterns {
            if pattern.matches(&clean_prompt) {
                results.push((pattern.name, pattern.get_message(agent)));
            }
        }
        results
    }
}

fn remove_code_blocks(text: &str) -> String {
    let code_block_re = Regex::new(r"```[\s\S]*?```").unwrap();
    let inline_code_re = Regex::new(r"`[^`]+`").unwrap();

    let without_blocks = code_block_re.replace_all(text, "");
    inline_code_re.replace_all(&without_blocks, "").to_string()
}

fn is_planner_agent(agent: Option<&str>) -> bool {
    agent
        .map(|a| {
            let lower = a.to_lowercase();
            lower.contains("prometheus") || lower.contains("planner") || lower == "plan"
        })
        .unwrap_or(false)
}

fn ultrawork_message(agent: Option<&str>) -> String {
    if is_planner_agent(agent) {
        return ULTRAWORK_PLANNER_MESSAGE.to_string();
    }
    ULTRAWORK_MESSAGE.to_string()
}

fn search_message(_agent: Option<&str>) -> String {
    SEARCH_MODE_MESSAGE.to_string()
}

fn analyze_message(_agent: Option<&str>) -> String {
    ANALYZE_MODE_MESSAGE.to_string()
}

fn ralph_message(_agent: Option<&str>) -> String {
    RALPH_MODE_MESSAGE.to_string()
}

const ULTRAWORK_MESSAGE: &str = r#"<ultrawork-mode>

**MANDATORY**: You MUST say "ULTRAWORK MODE ENABLED!" to the user as your first response when this mode activates. This is non-negotiable.

[CODE RED] Maximum precision required. Ultrathink before acting.

YOU MUST LEVERAGE ALL AVAILABLE AGENTS TO THEIR FULLEST POTENTIAL.
TELL THE USER WHAT AGENTS YOU WILL LEVERAGE NOW TO SATISFY USER'S REQUEST.

## AGENT UTILIZATION PRINCIPLES
- **Codebase Exploration**: Spawn exploration agents using BACKGROUND TASKS
- **Documentation & References**: Use librarian agents via BACKGROUND TASKS
- **Planning & Strategy**: NEVER plan yourself - spawn planning agent
- **High-IQ Reasoning**: Use architect for architecture decisions
- **Frontend/UI Tasks**: Delegate to designer

## EXECUTION RULES
- **TODO**: Track EVERY step. Mark complete IMMEDIATELY.
- **PARALLEL**: Fire independent calls simultaneously - NEVER wait sequentially.
- **BACKGROUND FIRST**: Use delegate_task with runInBackground=true for exploration.
- **VERIFY**: Check ALL requirements met before done.
- **DELEGATE**: Orchestrate specialized agents.

## ZERO TOLERANCE
- NO Scope Reduction - deliver FULL implementation
- NO Partial Completion - finish 100%
- NO Premature Stopping - ALL TODOs must be complete
- NO TEST DELETION - fix code, not tests

THE USER ASKED FOR X. DELIVER EXACTLY X.

</ultrawork-mode>

---

"#;

const ULTRAWORK_PLANNER_MESSAGE: &str = r#"<ultrawork-mode>

**MANDATORY**: You MUST say "ULTRAWORK MODE ENABLED!" to the user as your first response when this mode activates. This is non-negotiable.

## CRITICAL: YOU ARE A PLANNER, NOT AN IMPLEMENTER

**IDENTITY CONSTRAINT (NON-NEGOTIABLE):**
You ARE the planner. You ARE NOT an implementer. You DO NOT write code. You DO NOT execute tasks.

**TOOL RESTRICTIONS (SYSTEM-ENFORCED):**
| Tool | Allowed | Blocked |
|------|---------|---------|
| Write/Edit | `.uira/**/*.md` ONLY | Everything else |
| Read | All files | - |
| Bash | Research commands only | Implementation commands |
| Task | explore, librarian | - |

**WHEN USER ASKS YOU TO IMPLEMENT:**
REFUSE. Say: "I'm a planner. I create work plans, not implementations. The executor agents will implement after planning."

## CONTEXT GATHERING (MANDATORY BEFORE PLANNING)

**Before drafting ANY plan, gather context via explore/librarian agents.**

### Research Protocol
1. **Fire parallel background agents** for comprehensive context
2. **Wait for results** before planning - rushed plans fail
3. **Synthesize findings** into informed requirements

**NEVER plan blind. Context first, plan second.**

</ultrawork-mode>

---

"#;

const SEARCH_MODE_MESSAGE: &str = r#"<search-mode>
MAXIMIZE SEARCH EFFORT. Launch multiple background agents IN PARALLEL:
- explore agents (codebase patterns, file structures)
- librarian agents (remote repos, official docs, GitHub examples)
Plus direct tools: Grep, Glob, LSP
NEVER stop at first result - be exhaustive.
</search-mode>

---

"#;

const RALPH_MODE_MESSAGE: &str = r#"<ralph-mode>

[RALPH MODE ACTIVATED]

You are in RALPH mode - a self-referential work loop that continues until VERIFIED completion.

## CRITICAL RULES

1. **NEVER STOP** until the task is truly complete
2. **TRACK PROGRESS** using todo list - mark items complete as you go
3. **SIGNAL COMPLETION** with: <promise>TASK COMPLETE</promise>

## COMPLETION PROTOCOL

When you believe the task is done:
1. Verify all requirements are met
2. Ensure all tests pass (if applicable)
3. Check all todos are complete
4. Output: <promise>TASK COMPLETE</promise>

The system will verify your completion claim against configured goals.
If verification fails, you will be asked to continue.

## STATUS BLOCK (Optional)

You can output a status block to track progress:

---RALPH_STATUS---
STATUS: IN_PROGRESS | COMPLETE | BLOCKED
TASKS_COMPLETED_THIS_LOOP: <number>
FILES_MODIFIED: <number>
TESTS_STATUS: PASSING | FAILING | NOT_RUN
WORK_TYPE: IMPLEMENTATION | TESTING | DOCUMENTATION | REFACTORING | DEBUGGING
EXIT_SIGNAL: false | true
---END_RALPH_STATUS---

</ralph-mode>

---

"#;

const ANALYZE_MODE_MESSAGE: &str = r#"<analyze-mode>
ANALYSIS MODE. Gather context before diving deep:

CONTEXT GATHERING (parallel):
- 1-2 explore agents (codebase patterns, implementations)
- 1-2 librarian agents (if external library involved)
- Direct tools: Grep, Glob, LSP for targeted searches

IF COMPLEX (architecture, multi-system, debugging after 2+ failures):
- Consult architect agent for strategic guidance

SYNTHESIZE findings before proceeding.
</analyze-mode>

---

"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_ralph() {
        let detector = KeywordDetector::new();

        let result = detector.detect("ralph: do something", None);
        assert!(result.is_some());
        let msg = result.unwrap().message.unwrap();
        assert!(msg.contains("ralph-mode"));

        let result = detector.detect("don't stop until done", None);
        assert!(result.is_some());

        let result = detector.detect("must complete this task", None);
        assert!(result.is_some());
    }

    #[test]
    fn test_detect_ultrawork() {
        let detector = KeywordDetector::new();

        let result = detector.detect("ultrawork: do something", None);
        assert!(result.is_some());
        let msg = result.unwrap().message.unwrap();
        assert!(msg.contains("ULTRAWORK MODE ENABLED"));

        let result = detector.detect("ulw do this", None);
        assert!(result.is_some());
    }

    #[test]
    fn test_detect_search() {
        let detector = KeywordDetector::new();

        let result = detector.detect("search for files", None);
        assert!(result.is_some());
        let msg = result.unwrap().message.unwrap();
        assert!(msg.contains("search-mode"));

        let result = detector.detect("find the bug", None);
        assert!(result.is_some());
    }

    #[test]
    fn test_detect_analyze() {
        let detector = KeywordDetector::new();

        let result = detector.detect("analyze this code", None);
        assert!(result.is_some());
        let msg = result.unwrap().message.unwrap();
        assert!(msg.contains("analyze-mode"));
    }

    #[test]
    fn test_no_detection_in_code_blocks() {
        let detector = KeywordDetector::new();

        let result = detector.detect("Here's an example:\n```\nultrawork\n```", None);
        assert!(result.is_none());

        let result = detector.detect("Use `ultrawork` command", None);
        assert!(result.is_none());
    }

    #[test]
    fn test_planner_agent_message() {
        let detector = KeywordDetector::new();

        let result = detector.detect("ultrawork: plan this", Some("prometheus"));
        assert!(result.is_some());
        let msg = result.unwrap().message.unwrap();
        assert!(msg.contains("YOU ARE A PLANNER"));
    }

    #[test]
    fn test_priority_order() {
        let detector = KeywordDetector::new();

        let result = detector.detect("ultrawork search for files", None);
        assert!(result.is_some());
        let msg = result.unwrap().message.unwrap();
        assert!(msg.contains("ultrawork-mode"));
    }
}
