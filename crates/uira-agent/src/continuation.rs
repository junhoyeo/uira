//! Todo continuation injection
//!
//! Detects when the agent is about to stop but has incomplete todos,
//! and generates continuation messages to keep it working.

/// A continuation message to inject into the conversation
#[derive(Debug, Clone)]
pub struct ContinuationMessage {
    /// Optional system prompt injection
    pub system_injection: Option<String>,
    /// User message to inject
    pub user_injection: Option<String>,
}

/// Check if the agent's response text indicates it's trying to stop
pub fn is_completion_signal(text: &str) -> bool {
    let lower = text.to_lowercase();
    let negated_completion_phrases = [
        "not finished",
        "not done",
        "not complete",
        "not completed",
        "not all tasks",
        "isn't finished",
        "isn't done",
        "isn't complete",
        "is not finished",
        "is not done",
        "is not complete",
        "still incomplete",
    ];
    if negated_completion_phrases
        .iter()
        .any(|phrase| lower.contains(phrase))
    {
        return false;
    }

    let completion_phrases = [
        "<done/>",
        "i'm done",
        "i am done",
        "task complete",
        "all done",
        "finished",
        "completed all",
        "that's everything",
        "nothing more",
        "all tasks",
        "work is complete",
    ];
    completion_phrases
        .iter()
        .any(|phrase| lower.contains(phrase))
}

/// Check if todo continuation should be injected.
///
/// Returns true when the agent's output indicates completion
/// but there are still incomplete todo items.
pub fn check_todo_continuation(is_completion_signal: bool, has_incomplete_todos: bool) -> bool {
    is_completion_signal && has_incomplete_todos
}

/// Generate a continuation message for incomplete todos
pub fn generate_continuation(
    incomplete_count: usize,
    incomplete_summaries: &[String],
) -> ContinuationMessage {
    let items_list = incomplete_summaries
        .iter()
        .enumerate()
        .map(|(i, s)| format!("{}. {}", i + 1, s))
        .collect::<Vec<_>>()
        .join("\n");

    ContinuationMessage {
        system_injection: Some(
            "You have incomplete todo items. Do not stop until all items are completed. \
             Review the remaining items and continue working on them."
                .to_string(),
        ),
        user_injection: Some(format!(
            "You still have {} incomplete todo item(s). Please continue working on them:\n\n{}\n\nDo not stop until all items are completed.",
            incomplete_count, items_list
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_completion_signal_positive() {
        assert!(is_completion_signal("I'm done with all the changes."));
        assert!(is_completion_signal("The task complete successfully."));
        assert!(is_completion_signal("All done here."));
        assert!(is_completion_signal("I have finished the implementation."));
        assert!(is_completion_signal("Work is complete."));
    }

    #[test]
    fn test_is_completion_signal_negative() {
        assert!(!is_completion_signal("Here's the code for the feature."));
        assert!(!is_completion_signal("Let me implement this next."));
        assert!(!is_completion_signal("I'll fix the error now."));
        assert!(!is_completion_signal("Running the tests."));
        assert!(!is_completion_signal("I am not finished yet."));
        assert!(!is_completion_signal("Work is not complete."));
        assert!(!is_completion_signal("Not all tasks are done yet."));
    }

    #[test]
    fn test_is_completion_signal_case_insensitive() {
        assert!(is_completion_signal("<DONE/>"));
        assert!(is_completion_signal("I'M DONE with everything."));
        assert!(is_completion_signal("TASK COMPLETE."));
        assert!(is_completion_signal("ALL DONE."));
        assert!(is_completion_signal("Work Is Complete."));
    }

    #[test]
    fn test_generate_continuation_single() {
        let msg = generate_continuation(1, &["Fix login bug".to_string()]);
        let text = msg.user_injection.unwrap();
        assert!(text.contains("1 incomplete todo item(s)"));
        assert!(text.contains("1. Fix login bug"));
        assert!(msg.system_injection.is_some());
    }

    #[test]
    fn test_generate_continuation_multiple() {
        let items = vec![
            "Fix login bug".to_string(),
            "Add unit tests".to_string(),
            "Update docs".to_string(),
        ];
        let msg = generate_continuation(3, &items);
        let text = msg.user_injection.unwrap();
        assert!(text.contains("3 incomplete todo item(s)"));
        assert!(text.contains("1. Fix login bug"));
        assert!(text.contains("2. Add unit tests"));
        assert!(text.contains("3. Update docs"));
    }

    #[test]
    fn test_check_todo_continuation_logic() {
        // Should continue: completion signal + incomplete todos
        assert!(check_todo_continuation(true, true));
        // Should not: no completion signal
        assert!(!check_todo_continuation(false, true));
        // Should not: no incomplete todos
        assert!(!check_todo_continuation(true, false));
        // Should not: neither
        assert!(!check_todo_continuation(false, false));
    }

    #[test]
    fn test_generate_continuation_has_system_injection() {
        let msg = generate_continuation(1, &["Fix bug".to_string()]);
        assert!(msg.system_injection.is_some());
        let sys = msg.system_injection.unwrap();
        assert!(sys.contains("incomplete todo"));
        assert!(sys.contains("Do not stop"));
    }
}
