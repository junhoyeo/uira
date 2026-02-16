//! Uira Sandbox - Platform-native sandboxing
//!
//! This crate provides sandboxing capabilities for secure command execution:
//! - macOS: Seatbelt (sandbox-exec)
//! - Linux: Landlock + seccomp
//! - Windows: Restricted tokens (future)

mod error;
mod manager;
mod policy;
mod safety;

#[cfg(target_os = "linux")]
mod landlock;

#[cfg(target_os = "macos")]
mod seatbelt;

pub use error::SandboxError;
pub use manager::SandboxManager;
pub use policy::{SandboxPolicy, SandboxType};
pub use safety::{is_dangerous_command, is_safe_command};
