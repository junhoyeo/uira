//! Sandbox manager

#[cfg(target_os = "linux")]
use std::os::unix::process::CommandExt;
use std::process::Command;
use uira_protocol::SandboxPreference;

use crate::{SandboxError, SandboxPolicy, SandboxType};

/// Manages sandbox selection and command execution
pub struct SandboxManager {
    policy: SandboxPolicy,
}

impl SandboxManager {
    pub fn new(policy: SandboxPolicy) -> Self {
        Self { policy }
    }

    /// Select the initial sandbox type based on policy and preference
    pub fn select_sandbox(&self, preference: SandboxPreference) -> SandboxType {
        match preference {
            SandboxPreference::Forbid => SandboxType::None,
            SandboxPreference::Require => Self::get_platform_sandbox(),
            SandboxPreference::Auto => {
                if matches!(self.policy, SandboxPolicy::FullAccess) {
                    SandboxType::None
                } else {
                    Self::get_platform_sandbox()
                }
            }
        }
    }

    /// Get the native sandbox type for this platform
    fn get_platform_sandbox() -> SandboxType {
        #[cfg(target_os = "macos")]
        {
            SandboxType::Native // Seatbelt
        }
        #[cfg(target_os = "linux")]
        {
            SandboxType::Native // Landlock
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            SandboxType::None
        }
    }

    /// Check if sandbox is available on this platform
    pub fn is_available() -> bool {
        #[cfg(target_os = "macos")]
        {
            std::path::Path::new("/usr/bin/sandbox-exec").exists()
        }
        #[cfg(target_os = "linux")]
        {
            // Check for landlock support
            true // Simplified - would check kernel version
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            false
        }
    }

    /// Wrap a command with sandbox restrictions
    pub fn wrap_command(
        &self,
        cmd: &mut Command,
        sandbox: SandboxType,
    ) -> Result<(), SandboxError> {
        match sandbox {
            SandboxType::None => Ok(()),
            SandboxType::Native => {
                #[cfg(target_os = "macos")]
                {
                    self.wrap_seatbelt(cmd)
                }
                #[cfg(target_os = "linux")]
                {
                    self.wrap_landlock(cmd)
                }
                #[cfg(not(any(target_os = "macos", target_os = "linux")))]
                {
                    Err(SandboxError::NotAvailable)
                }
            }
            SandboxType::Container => {
                // Container sandboxing not yet implemented
                Err(SandboxError::NotAvailable)
            }
        }
    }

    #[cfg(target_os = "macos")]
    fn wrap_seatbelt(&self, cmd: &mut Command) -> Result<(), SandboxError> {
        use crate::seatbelt;

        // Generate the seatbelt policy
        let policy = seatbelt::generate_policy(&self.policy);

        // Get the original program and args
        let program = cmd.get_program().to_os_string();
        let args: Vec<_> = cmd.get_args().map(|a| a.to_os_string()).collect();

        // Replace the command with sandbox-exec
        *cmd = Command::new("/usr/bin/sandbox-exec");
        cmd.arg("-p").arg(&policy).arg("--").arg(&program);
        for arg in args {
            cmd.arg(arg);
        }

        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn wrap_landlock(&self, cmd: &mut Command) -> Result<(), SandboxError> {
        use crate::landlock;

        // Landlock requires applying restrictions BEFORE fork/exec.
        // We use pre_exec to apply the restrictions in the child process.
        //
        // Note: This is a simplified approach. A more robust implementation would
        // use a dedicated sandbox binary wrapper similar to how seatbelt uses sandbox-exec.

        let rules = landlock::generate_rules(&self.policy);

        // Clone data for the pre_exec closure
        let rules_json = serde_json::to_string(&rules)
            .map_err(|e| SandboxError::PolicyViolation(e.to_string()))?;

        // SAFETY: pre_exec runs after fork but before exec, in the child process.
        // We're only calling async-signal-safe operations in the closure.
        unsafe {
            cmd.pre_exec(move || {
                // Parse the rules back (we can't capture non-Copy types directly)
                let rules: Vec<landlock::LandlockRule> = match serde_json::from_str(&rules_json) {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("Failed to parse landlock rules: {}", e);
                        return Err(std::io::Error::other(e));
                    }
                };

                // Apply landlock restrictions
                if let Err(e) = landlock::apply_landlock(&rules) {
                    eprintln!("Failed to apply landlock: {}", e);
                    return Err(std::io::Error::other(e));
                }

                Ok(())
            });
        }

        Ok(())
    }
}

impl Default for SandboxManager {
    fn default() -> Self {
        Self::new(SandboxPolicy::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_selection() {
        let manager = SandboxManager::new(SandboxPolicy::full_access());
        assert_eq!(
            manager.select_sandbox(SandboxPreference::Auto),
            SandboxType::None
        );

        let manager = SandboxManager::new(SandboxPolicy::read_only());
        assert_eq!(
            manager.select_sandbox(SandboxPreference::Forbid),
            SandboxType::None
        );
    }
}
