//! Rate Limit Wait - Automatic detection and resume for rate-limited Claude sessions
//!
//! This module provides automatic monitoring and recovery from Claude API rate limits
//! by detecting blocked tmux panes and automatically resuming them when limits reset.
//!
//! ## Features
//!
//! - **Automatic Detection**: Scans tmux panes for rate limit error patterns
//! - **Smart Resume**: Automatically sends resume commands when limits clear
//! - **Daemon Mode**: Background process that continuously monitors and recovers
//! - **Status Tracking**: Maintains state of blocked panes and recovery attempts
//!
//! ## Usage
//!
//! ```rust,no_run
//! use uira_features::rate_limit_wait::{DaemonConfig, start_daemon, get_daemon_status};
//!
//! # async fn example() {
//! // Start the rate limit monitor daemon
//! let config = DaemonConfig::default();
//! let response = start_daemon(config.clone());
//! println!("Daemon started: {:?}", response);
//!
//! // Check status
//! let status = get_daemon_status(&config);
//! println!("Status: {:?}", status);
//! # }
//! ```
//!
//! ## Detection Patterns
//!
//! The module detects the following rate limit indicators:
//! - "Rate limit reached"
//! - "Please wait"
//! - "Usage limit"
//! - "429" (HTTP status code)
//! - "Too many requests"
//!
//! ## State Files
//!
//! All state is stored in `~/.uira/state/`:
//! - `rate-limit-daemon.json` - Current daemon state
//! - `rate-limit-daemon.pid` - Process ID file
//! - `rate-limit-daemon.log` - Daemon activity log

pub mod daemon;
pub mod monitor;
pub mod tmux;
pub mod types;

pub use daemon::{
    detect_blocked_panes, get_daemon_status, is_daemon_running, read_daemon_state,
    run_daemon_foreground, start_daemon, stop_daemon,
};
pub use monitor::{check_rate_limit_status, format_rate_limit_status, format_time_until_reset};
pub use tmux::{
    capture_pane_content, is_inside_tmux, is_tmux_available, list_tmux_panes,
    scan_for_blocked_panes, send_resume_sequence,
};
pub use types::{
    BlockedPane, DaemonConfig, DaemonResponse, DaemonState, RateLimitStatus, TmuxPane,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_exports() {
        // Verify all public APIs are accessible
        let config = DaemonConfig::default();
        assert!(config.poll_interval_ms > 0);

        let state = DaemonState::default();
        assert!(!state.is_running);
    }
}
