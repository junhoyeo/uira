use super::monitor::{check_rate_limit_status, format_rate_limit_status};
use super::tmux::{scan_for_blocked_panes, send_resume_sequence};
use super::types::{DaemonConfig, DaemonResponse, DaemonState};
use anyhow::Result;
use chrono::Utc;
use std::fs;
use std::io::Write;
use std::process;
use tokio::time::{sleep, Duration};

/// Read daemon state from file
pub fn read_daemon_state(config: &DaemonConfig) -> Option<DaemonState> {
    let content = fs::read_to_string(&config.state_file_path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Write daemon state to file
fn write_daemon_state(config: &DaemonConfig, state: &DaemonState) -> Result<()> {
    // Ensure directory exists
    if let Some(parent) = config.state_file_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let content = serde_json::to_string_pretty(state)?;
    fs::write(&config.state_file_path, content)?;
    Ok(())
}

/// Check if daemon is currently running
pub fn is_daemon_running(config: &DaemonConfig) -> bool {
    // Check PID file
    if let Ok(pid_str) = fs::read_to_string(&config.pid_file_path) {
        if let Ok(pid) = pid_str.trim().parse::<u32>() {
            // Check if process is actually running
            #[cfg(unix)]
            {
                use std::process::Command;
                return Command::new("kill")
                    .args(["-0", &pid.to_string()])
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false);
            }
            #[cfg(not(unix))]
            {
                // On non-Unix, just check if PID file exists
                return true;
            }
        }
    }
    false
}

/// Start daemon process
pub fn start_daemon(config: DaemonConfig) -> DaemonResponse {
    if is_daemon_running(&config) {
        return DaemonResponse::Error {
            message: "Daemon is already running".to_string(),
        };
    }

    // Ensure directories exist
    for path in &[
        &config.state_file_path,
        &config.pid_file_path,
        &config.log_file_path,
    ] {
        if let Some(parent) = path.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                return DaemonResponse::Error {
                    message: format!("Failed to create state directory: {}", e),
                };
            }
        }
    }

    // Fork daemon process
    let pid = process::id();

    // Write PID file
    if let Err(e) = fs::write(&config.pid_file_path, pid.to_string()) {
        return DaemonResponse::Error {
            message: format!("Failed to write PID file: {}", e),
        };
    }

    // Initialize state
    let state = DaemonState {
        is_running: true,
        pid: Some(pid),
        started_at: Some(Utc::now()),
        ..Default::default()
    };

    if let Err(e) = write_daemon_state(&config, &state) {
        return DaemonResponse::Error {
            message: format!("Failed to write initial state: {}", e),
        };
    }

    // In a real implementation, this would fork and run in background
    // For now, we'll return success and expect caller to run daemon loop

    DaemonResponse::Started {
        pid,
        message: format!("Daemon started with PID {}", pid),
    }
}

/// Main daemon loop (runs in foreground for this implementation)
pub async fn run_daemon_foreground(config: DaemonConfig) -> Result<()> {
    let pid = process::id();
    log_message(&config, &format!("Daemon starting with PID {}", pid));

    loop {
        // Check if we should stop (PID file removed)
        if !config.pid_file_path.exists() {
            log_message(&config, "PID file removed, stopping daemon");
            break;
        }

        // Read current state
        let mut state = read_daemon_state(&config).unwrap_or_default();
        state.last_poll_at = Some(Utc::now());

        // Check rate limit status
        match check_rate_limit_status().await {
            Some(rate_status) => {
                state.rate_limit_status = Some(rate_status.clone());

                if config.verbose {
                    log_message(&config, &format_rate_limit_status(&rate_status));
                }

                // If rate limit has cleared, try to resume blocked panes
                if !rate_status.is_limited {
                    let blocked_count = state.blocked_panes.len();
                    if blocked_count > 0 {
                        log_message(
                            &config,
                            &format!(
                                "Rate limit cleared, resuming {} blocked panes",
                                blocked_count
                            ),
                        );

                        // Try to resume each blocked pane
                        for pane in &mut state.blocked_panes {
                            if !pane.resume_attempted {
                                state.total_resume_attempts += 1;
                                pane.resume_attempted = true;

                                if send_resume_sequence(&pane.id) {
                                    pane.resume_successful = true;
                                    state.successful_resumes += 1;
                                    state.resumed_pane_ids.push(pane.id.clone());
                                    log_message(
                                        &config,
                                        &format!(
                                            "Resumed pane {} in session {}",
                                            pane.id, pane.session
                                        ),
                                    );
                                } else {
                                    log_message(
                                        &config,
                                        &format!("Failed to resume pane {}", pane.id),
                                    );
                                }
                            }
                        }

                        // Clear successfully resumed panes
                        state.blocked_panes.retain(|p| !p.resume_successful);
                    }
                }
            }
            None => {
                state.error_count += 1;
                state.last_error = Some("Failed to check rate limit status".to_string());
                log_message(&config, "Failed to check rate limit status");
            }
        }

        // Scan for newly blocked panes
        let newly_blocked = scan_for_blocked_panes(config.pane_lines_to_capture);
        for blocked in newly_blocked {
            // Check if this pane is already tracked
            if !state.blocked_panes.iter().any(|p| p.id == blocked.id)
                && !state.resumed_pane_ids.contains(&blocked.id)
            {
                log_message(
                    &config,
                    &format!(
                        "Detected blocked pane {} in session {}",
                        blocked.id, blocked.session
                    ),
                );
                state.blocked_panes.push(blocked);
            }
        }

        // Write updated state
        if let Err(e) = write_daemon_state(&config, &state) {
            log_message(&config, &format!("Failed to write state: {}", e));
        }

        // Sleep until next poll
        sleep(Duration::from_millis(config.poll_interval_ms)).await;
    }

    log_message(&config, "Daemon stopped");
    Ok(())
}

/// Stop daemon process
pub fn stop_daemon(config: &DaemonConfig) -> DaemonResponse {
    if !is_daemon_running(config) {
        return DaemonResponse::Error {
            message: "Daemon is not running".to_string(),
        };
    }

    // Read PID and kill process
    if let Ok(pid_str) = fs::read_to_string(&config.pid_file_path) {
        if let Ok(pid) = pid_str.trim().parse::<u32>() {
            #[cfg(unix)]
            {
                use std::process::Command;
                let _ = Command::new("kill")
                    .args(["-TERM", &pid.to_string()])
                    .status();
            }
        }
    }

    // Remove PID file
    let _ = fs::remove_file(&config.pid_file_path);

    // Update state
    if let Some(mut state) = read_daemon_state(config) {
        state.is_running = false;
        let _ = write_daemon_state(config, &state);
    }

    DaemonResponse::Stopped {
        message: "Daemon stopped".to_string(),
    }
}

/// Get current daemon status
pub fn get_daemon_status(config: &DaemonConfig) -> DaemonResponse {
    match read_daemon_state(config) {
        Some(state) => DaemonResponse::Status { state },
        None => DaemonResponse::Error {
            message: "No daemon state found".to_string(),
        },
    }
}

/// Detect blocked panes without starting daemon
pub async fn detect_blocked_panes(config: &DaemonConfig) -> DaemonResponse {
    let panes = scan_for_blocked_panes(config.pane_lines_to_capture);
    DaemonResponse::BlockedPanesDetected { panes }
}

/// Log message to daemon log file
fn log_message(config: &DaemonConfig, message: &str) {
    let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S");
    let log_line = format!("[{}] {}\n", timestamp, message);

    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&config.log_file_path)
    {
        let _ = file.write_all(log_line.as_bytes());
    }

    if config.verbose {
        print!("{}", log_line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_daemon_state_read_write() {
        let temp_dir = TempDir::new().unwrap();
        let mut config = DaemonConfig::default();
        config.state_file_path = temp_dir.path().join("state.json");

        let mut state = DaemonState::default();
        state.is_running = true;
        state.pid = Some(12345);

        write_daemon_state(&config, &state).unwrap();

        let read_state = read_daemon_state(&config).unwrap();
        assert_eq!(read_state.pid, Some(12345));
        assert!(read_state.is_running);
    }

    #[test]
    fn test_is_daemon_running_no_pid_file() {
        let temp_dir = TempDir::new().unwrap();
        let mut config = DaemonConfig::default();
        config.pid_file_path = temp_dir.path().join("daemon.pid");

        assert!(!is_daemon_running(&config));
    }
}
