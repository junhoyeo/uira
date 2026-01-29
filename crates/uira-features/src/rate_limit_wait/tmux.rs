use super::types::{BlockedPane, TmuxPane};
use chrono::Utc;
use std::process::Command;

/// Check if tmux is installed and available
pub fn is_tmux_available() -> bool {
    Command::new("tmux")
        .arg("-V")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Check if running inside a tmux session
pub fn is_inside_tmux() -> bool {
    std::env::var("TMUX").is_ok()
}

/// List all tmux panes across all sessions
pub fn list_tmux_panes() -> Vec<TmuxPane> {
    let output = match Command::new("tmux")
        .args([
            "list-panes",
            "-a",
            "-F",
            "#{pane_id}|#{session_name}|#{window_index}|#{pane_active}",
        ])
        .output()
    {
        Ok(output) if output.status.success() => output,
        _ => return Vec::new(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() == 4 {
                Some(TmuxPane {
                    id: parts[0].to_string(),
                    session: parts[1].to_string(),
                    window_index: parts[2].parse().unwrap_or(0),
                    active: parts[3] == "1",
                })
            } else {
                None
            }
        })
        .collect()
}

/// Capture content from a specific tmux pane
pub fn capture_pane_content(pane_id: &str, lines: usize) -> String {
    let output = match Command::new("tmux")
        .args([
            "capture-pane",
            "-p",
            "-t",
            pane_id,
            "-S",
            &format!("-{}", lines),
        ])
        .output()
    {
        Ok(output) if output.status.success() => output,
        _ => return String::new(),
    };

    String::from_utf8_lossy(&output.stdout).to_string()
}

/// Scan all tmux panes for rate limit blocking patterns
pub fn scan_for_blocked_panes(lines: usize) -> Vec<BlockedPane> {
    let panes = list_tmux_panes();
    let mut blocked = Vec::new();

    // Patterns that indicate a blocked Claude session
    let blocking_patterns = [
        "Rate limit reached",
        "Please wait",
        "Usage limit",
        "429",
        "Too many requests",
        "rate_limit_error",
        "You've hit a rate limit",
    ];

    for pane in panes {
        let content = capture_pane_content(&pane.id, lines);
        let content_lower = content.to_lowercase();

        // Check if any blocking pattern is present
        let is_blocked = blocking_patterns
            .iter()
            .any(|pattern| content_lower.contains(&pattern.to_lowercase()));

        if is_blocked {
            blocked.push(BlockedPane {
                id: pane.id.clone(),
                session: pane.session.clone(),
                window_index: pane.window_index,
                first_detected_at: Utc::now(),
                resume_attempted: false,
                resume_successful: false,
            });
        }
    }

    blocked
}

/// Send resume sequence to a tmux pane (Enter key)
pub fn send_resume_sequence(pane_id: &str) -> bool {
    // Send Enter key to attempt resume
    let result = Command::new("tmux")
        .args(["send-keys", "-t", pane_id, "Enter"])
        .status();

    result.map(|status| status.success()).unwrap_or(false)
}

/// Send a custom command to a tmux pane
pub fn send_keys_to_pane(pane_id: &str, keys: &str) -> bool {
    let result = Command::new("tmux")
        .args(["send-keys", "-t", pane_id, keys])
        .status();

    result.map(|status| status.success()).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tmux_available() {
        // This test will pass if tmux is installed
        // Can't assert true/false as it depends on environment
        let _ = is_tmux_available();
    }

    #[test]
    fn test_inside_tmux() {
        // This depends on whether test is run inside tmux
        let _ = is_inside_tmux();
    }
}
