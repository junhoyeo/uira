//! OpenCode server auto-start logic for MCP server.
//!
//! This module provides functionality to automatically start and manage
//! the OpenCode server, ensuring it's running before making API calls.

use anyhow::{Context, Result};
use std::process::{Command, Stdio};
use std::time::Duration;

/// Default OpenCode server port
#[allow(dead_code)]
pub const DEFAULT_OPENCODE_PORT: u16 = 4096;

/// Manager for OpenCode server lifecycle.
///
/// Handles health checks and auto-starting the OpenCode server when needed.
/// Uses async reqwest client for non-blocking health checks.
pub struct OpencodeServerManager {
    host: String,
    port: u16,
    server_was_started: bool,
    client: reqwest::Client,
}

impl OpencodeServerManager {
    /// Create a new server manager with the given host and port.
    pub fn new(host: String, port: u16) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            host,
            port,
            server_was_started: false,
            client,
        }
    }

    /// Create a new server manager with default settings (localhost:4096).
    #[allow(dead_code)]
    pub fn with_defaults() -> Self {
        Self::new("127.0.0.1".to_string(), DEFAULT_OPENCODE_PORT)
    }

    /// Check if the OpenCode server is running by hitting the health endpoint.
    pub async fn is_server_running(&self) -> bool {
        self.client
            .get(format!("http://{}:{}/health", self.host, self.port))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    /// Start the OpenCode server process.
    ///
    /// Spawns `opencode serve` as a detached background process.
    pub fn start_server(&mut self) -> Result<()> {
        Command::new("opencode")
            .args(["serve"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("Failed to start OpenCode server. Is opencode installed?")?;

        self.server_was_started = true;
        Ok(())
    }

    /// Ensure the OpenCode server is running.
    ///
    /// If the server is not running, starts it and waits up to 15 seconds
    /// for it to become healthy.
    ///
    /// # Returns
    ///
    /// - `Ok(())` if the server is running or was successfully started
    /// - `Err` if the server could not be started or didn't become healthy
    pub async fn ensure_opencode_server(&mut self) -> Result<()> {
        // Check if already running
        if self.is_server_running().await {
            return Ok(());
        }

        // Start the server
        self.start_server()?;

        // Wait for server to become healthy (up to 15 seconds)
        // 30 iterations × 500ms = 15 seconds
        for _ in 0..30 {
            tokio::time::sleep(Duration::from_millis(500)).await;
            if self.is_server_running().await {
                return Ok(());
            }
        }

        anyhow::bail!(
            "OpenCode server failed to start within 15 seconds. \
             Please check if opencode is installed and working correctly."
        )
    }

    /// Returns whether this manager started the server.
    #[allow(dead_code)]
    pub fn server_was_started(&self) -> bool {
        self.server_was_started
    }

    /// Get the server URL.
    #[allow(dead_code)]
    pub fn server_url(&self) -> String {
        format!("http://{}:{}", self.host, self.port)
    }

    /// Get the configured port.
    #[allow(dead_code)]
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Get the configured host.
    #[allow(dead_code)]
    pub fn host(&self) -> &str {
        &self.host
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_url() {
        let manager = OpencodeServerManager::new("localhost".to_string(), 8080);
        assert_eq!(manager.server_url(), "http://localhost:8080");
    }

    #[test]
    fn test_default_port() {
        let manager = OpencodeServerManager::with_defaults();
        assert_eq!(manager.port(), DEFAULT_OPENCODE_PORT);
        assert_eq!(manager.host(), "127.0.0.1");
    }

    #[test]
    fn test_server_was_started_initially_false() {
        let manager = OpencodeServerManager::with_defaults();
        assert!(!manager.server_was_started());
    }
}
