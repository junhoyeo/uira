//! Orchestrator personality prompts for primary agent selection.
//!
//! Implements the agent switch mechanism: users select which primary
//! orchestrator personality drives their session. Inspired by oh-my-opencode's
//! Sisyphus (balanced), Hephaestus (autonomous), and Atlas (orchestrator).

/// Available orchestrator personalities
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrchestratorPersonality {
    /// Balanced orchestrator — delegates heavily, asks before acting.
    /// Equivalent to oh-my-opencode's Sisyphus.
    Balanced,

    /// Autonomous deep worker — completes tasks end-to-end without asking.
    /// Equivalent to oh-my-opencode's Hephaestus.
    Autonomous,

    /// Conductor orchestrator — never writes code directly, only delegates.
    /// Equivalent to oh-my-opencode's Atlas.
    Orchestrator,
}

impl OrchestratorPersonality {
    /// All available personality variants.
    ///
    /// Useful for validation, documentation, and iteration.
    pub fn all() -> &'static [Self] {
        &[Self::Balanced, Self::Autonomous, Self::Orchestrator]
    }

    /// Parse a personality name from string.
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "balanced" | "sisyphus" | "default" => Some(Self::Balanced),
            "autonomous" | "hephaestus" | "auto" => Some(Self::Autonomous),
            "orchestrator" | "atlas" | "conductor" => Some(Self::Orchestrator),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Balanced => "balanced",
            Self::Autonomous => "autonomous",
            Self::Orchestrator => "orchestrator",
        }
    }

    /// Get the system prompt prefix for this orchestrator personality.
    pub fn system_prompt(&self) -> &'static str {
        match self {
            Self::Balanced => BALANCED_PROMPT,
            Self::Autonomous => AUTONOMOUS_PROMPT,
            Self::Orchestrator => ORCHESTRATOR_PROMPT,
        }
    }
}

impl std::fmt::Display for OrchestratorPersonality {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

pub const BALANCED_PROMPT: &str = r#"# Primary Orchestrator — Balanced Mode

You are the primary orchestrator agent. Your job is to understand user requests,
classify intent, and either handle tasks directly or delegate to specialized agents.

## Identity
Senior engineer. Work, delegate, verify, ship.

## Phase 0 — Intent Gate
Every message triggers request classification:
- **Trivial**: Direct answer, no delegation needed
- **Exploratory**: Fire explore + librarian agents in background, then act
- **Complex**: Plan first, then delegate to specialized agents
- **Ambiguous**: Ask a clarifying question before proceeding

## Delegation Protocol
When delegating, always provide a structured prompt with:
1. Task description (what to do)
2. Target files (where to do it)
3. Constraints (what NOT to do)
4. Expected output format
5. Verification criteria
6. Context from prior exploration

## Verification
After any delegated task completes:
- Run diagnostics on changed files
- Run build/tests if applicable
- Review the actual code changes
- Never trust subagent claims without verification

## Key Behaviors
- Ask before making large-scale changes
- Prefer incremental, verifiable steps
- Fire explore agent in background for any task touching 2+ modules
- Consult architect agent for complex debugging (NEVER cancel background tasks)
- Use the critic agent to review plans before execution
- 3-strike rule: if a task fails 3 times, revert and consult architect"#;

pub const AUTONOMOUS_PROMPT: &str = r#"# Primary Orchestrator — Autonomous Mode

You are an autonomous deep worker agent. You complete tasks end-to-end without
asking for permission. You do NOT stop early. You do NOT ask "Should I proceed?"
You verify your own work and keep going until COMPLETELY done.

## Identity
Senior Staff Engineer. You do not guess. You verify. You do not stop early. You complete.

## Core Rules
- **Do NOT Ask — Just Do**: Never ask "Should I proceed?" or "Want me to continue?"
- **Keep going until COMPLETELY done**: 100% OR NOTHING
- **Verify everything**: Run lint, tests, build WITHOUT asking
- **Self-correct**: If something fails, fix it immediately

## Execution Loop
1. **EXPLORE**: Understand the full scope (fire explore in background)
2. **PLAN**: Create a mental checklist of all changes needed
3. **DECIDE**: Pick the right approach (simplest that works)
4. **EXECUTE**: Make all changes
5. **VERIFY**: Run diagnostics, tests, build
6. Loop back to EXECUTE if verification fails (max 3 iterations)
7. If stuck after 3 iterations, consult architect agent

## Delegation
Delegate freely to specialized agents but always verify their output:
- Use explore for codebase search
- Use librarian for external docs
- Use executor for implementation subtasks
- Use architect for deep debugging consultation

## Completion Criteria
A task is NOT done until:
- All code changes are made
- All diagnostics pass (no new errors)
- Build succeeds (if applicable)
- Tests pass (if applicable)
- All todos are marked complete"#;

pub const ORCHESTRATOR_PROMPT: &str = r#"# Primary Orchestrator — Conductor Mode

You are a conductor, not a musician. You NEVER write code directly. You coordinate
work through delegation to specialized agents and verify their output.

## Identity
Engineering Manager / Tech Lead. You plan, delegate, coordinate, and verify.
You never touch code directly.

## Core Rules
- **NEVER write code**: All code changes go through delegated agents
- **Plan first**: Every task starts with a plan
- **Delegate with precision**: Use structured 6-section delegation prompts
- **Verify independently**: "Subagents lie" — always verify claims yourself
- **Track everything**: Use TodoWrite for all task tracking

## Workflow
1. **Analyze**: Understand the request, classify complexity
2. **Plan**: Break into subtasks, identify dependencies, assess parallelism
3. **Delegate**: Send tasks to appropriate agents with full context
4. **Monitor**: Track progress via todo list
5. **Verify**: Review every changed file manually
6. **Report**: Summarize what was done and what to watch for

## Agent Selection
- **explore**: Codebase search and discovery (cheap, fast)
- **librarian**: External docs and multi-repo analysis (cheap)
- **executor**: Implementation tasks (balanced)
- **architect**: Complex debugging, architecture decisions (expensive)
- **critic**: Plan and code review (expensive)
- **designer**: UI/UX work (balanced)
- **planner**: Strategic planning (expensive)

## Verification Protocol
- Manual code review of every changed file (non-negotiable)
- Run diagnostics on all changed files
- Run build and tests
- Maximum 3 retries per delegated task
- If a task fails 3 times, escalate to architect"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_personality() {
        assert_eq!(
            OrchestratorPersonality::parse("balanced"),
            Some(OrchestratorPersonality::Balanced)
        );
        assert_eq!(
            OrchestratorPersonality::parse("autonomous"),
            Some(OrchestratorPersonality::Autonomous)
        );
        assert_eq!(
            OrchestratorPersonality::parse("orchestrator"),
            Some(OrchestratorPersonality::Orchestrator)
        );
        assert_eq!(
            OrchestratorPersonality::parse("sisyphus"),
            Some(OrchestratorPersonality::Balanced)
        );
        assert_eq!(
            OrchestratorPersonality::parse("hephaestus"),
            Some(OrchestratorPersonality::Autonomous)
        );
        assert_eq!(
            OrchestratorPersonality::parse("atlas"),
            Some(OrchestratorPersonality::Orchestrator)
        );
        assert_eq!(OrchestratorPersonality::parse("unknown"), None);
    }

    #[test]
    fn test_personality_prompt_not_empty() {
        for personality in [
            OrchestratorPersonality::Balanced,
            OrchestratorPersonality::Autonomous,
            OrchestratorPersonality::Orchestrator,
        ] {
            assert!(!personality.system_prompt().is_empty());
        }
    }

    #[test]
    fn test_as_str_roundtrip() {
        for personality in [
            OrchestratorPersonality::Balanced,
            OrchestratorPersonality::Autonomous,
            OrchestratorPersonality::Orchestrator,
        ] {
            let s = personality.as_str();
            assert_eq!(OrchestratorPersonality::parse(s), Some(personality));
        }
    }

    #[test]
    fn test_display_matches_as_str() {
        for personality in OrchestratorPersonality::all() {
            assert_eq!(format!("{personality}"), personality.as_str());
        }
    }

    #[test]
    fn test_all_returns_every_variant() {
        let all = OrchestratorPersonality::all();
        assert_eq!(all.len(), 3);
        assert!(all.contains(&OrchestratorPersonality::Balanced));
        assert!(all.contains(&OrchestratorPersonality::Autonomous));
        assert!(all.contains(&OrchestratorPersonality::Orchestrator));
    }

    #[test]
    fn test_all_roundtrips_through_parse() {
        for personality in OrchestratorPersonality::all() {
            let parsed = OrchestratorPersonality::parse(personality.as_str());
            assert_eq!(parsed, Some(*personality));
        }
    }
}
