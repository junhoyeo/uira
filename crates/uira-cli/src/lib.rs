//! Uira CLI - Command-line interface
//!
//! This crate provides the `uira` binary with multiple modes:
//! - Interactive mode (default): Full TUI experience
//! - Exec mode: Headless execution for scripts
//! - Serve mode: API server for integration

pub mod commands;
pub mod config;

pub use commands::{Cli, CliMode, Commands};
pub use config::CliConfig;
