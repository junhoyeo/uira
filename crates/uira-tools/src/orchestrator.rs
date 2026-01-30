//! Tool orchestrator for approval → sandbox → escalate flow

use std::sync::Arc;
use tokio::sync::mpsc;
use uira_protocol::{ApprovalRequirement, ReviewDecision, ToolOutput};
use uira_sandbox::{SandboxManager, SandboxPolicy, SandboxType};

use crate::comment_hook::CommentChecker;
use crate::{BoxedTool, ToolContext, ToolError, ToolRouter};

/// Options for tool execution
#[derive(Debug, Clone, Default)]
pub struct RunOptions {
    /// Skip approval check (approval already handled by caller)
    pub skip_approval: bool,
    /// Skip sandbox (run directly)
    pub skip_sandbox: bool,
}

impl RunOptions {
    /// Create options that skip approval (for when caller handles it)
    pub fn skip_approval() -> Self {
        Self {
            skip_approval: true,
            skip_sandbox: false,
        }
    }

    /// Create options that skip both approval and sandbox
    pub fn skip_all() -> Self {
        Self {
            skip_approval: true,
            skip_sandbox: true,
        }
    }
}

/// Request for user approval
#[derive(Debug)]
pub struct PendingApproval {
    pub id: String,
    pub tool_name: String,
    pub input: serde_json::Value,
    pub reason: String,
    pub response_tx: tokio::sync::oneshot::Sender<ReviewDecision>,
}

/// Orchestrator for tool execution with approval and sandboxing
pub struct ToolOrchestrator {
    router: Arc<ToolRouter>,
    sandbox_manager: SandboxManager,
    comment_checker: CommentChecker,
    approval_tx: mpsc::Sender<PendingApproval>,
    approval_rx: Option<mpsc::Receiver<PendingApproval>>,
    full_auto: bool,
    enable_comment_warnings: bool,
}

impl ToolOrchestrator {
    pub fn new(router: Arc<ToolRouter>, sandbox_policy: SandboxPolicy) -> Self {
        let (tx, rx) = mpsc::channel(100);
        Self {
            router,
            sandbox_manager: SandboxManager::new(sandbox_policy),
            comment_checker: CommentChecker::new(),
            approval_tx: tx,
            approval_rx: Some(rx),
            full_auto: false,
            enable_comment_warnings: true,
        }
    }

    /// Set full-auto mode (skip all approvals)
    pub fn with_full_auto(mut self, full_auto: bool) -> Self {
        self.full_auto = full_auto;
        self
    }

    /// Enable or disable comment warnings
    pub fn with_comment_warnings(mut self, enabled: bool) -> Self {
        self.enable_comment_warnings = enabled;
        self
    }

    /// Take the approval receiver for handling in UI
    pub fn take_approval_receiver(&mut self) -> Option<mpsc::Receiver<PendingApproval>> {
        self.approval_rx.take()
    }

    /// Run a tool with the approval → sandbox → escalate flow
    pub async fn run(
        &self,
        tool_name: &str,
        input: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        self.run_with_options(tool_name, input, ctx, RunOptions::default())
            .await
    }

    pub async fn run_with_options(
        &self,
        tool_name: &str,
        input: serde_json::Value,
        ctx: &ToolContext,
        options: RunOptions,
    ) -> Result<ToolOutput, ToolError> {
        // Check if tool is a direct tool or provider-backed
        let direct_tool = self.router.get(tool_name);

        // Provider-backed tools (e.g., delegate_task, background_output) handle their own
        // security through the provider implementation. They don't need orchestrator-level
        // approval/sandbox because:
        // 1. delegate_task spawns subagents with their own configs
        // 2. background_output only reads task results, no file/command access
        // 3. Providers implement their own validation in dispatch()
        if direct_tool.is_none() {
            return self.router.dispatch(tool_name, input, ctx).await;
        }

        let tool = direct_tool.unwrap();

        // 1. Check approval requirement (unless skipped by options)
        if !options.skip_approval {
            let requirement = tool.approval_requirement(&input);

            match requirement {
                ApprovalRequirement::Skip { bypass_sandbox } => {
                    // Proceed directly
                    if bypass_sandbox || options.skip_sandbox {
                        return self.execute_without_sandbox(tool, input, ctx).await;
                    }
                }
                ApprovalRequirement::NeedsApproval { reason } => {
                    if !self.full_auto && !ctx.full_auto {
                        // Request approval
                        let decision = self.request_approval(tool_name, &input, &reason).await?;
                        if decision.is_denied() {
                            return Err(ToolError::ExecutionFailed {
                                message: "Approval denied by user".to_string(),
                            });
                        }
                        // If edited, use the new input
                        if let ReviewDecision::Edit { new_input } = decision {
                            return self.execute_with_sandbox(tool, new_input, ctx).await;
                        }
                    }
                }
                ApprovalRequirement::Forbidden { reason } => {
                    return Err(ToolError::ExecutionFailed {
                        message: format!("Tool execution forbidden: {}", reason),
                    });
                }
            }
        }

        // 2. Select sandbox and execute (or skip sandbox if requested)
        let mut output = if options.skip_sandbox {
            self.execute_without_sandbox(tool, input.clone(), ctx)
                .await?
        } else {
            self.execute_with_sandbox(tool, input.clone(), ctx).await?
        };

        // 3. Post-execution: Check for comments in write operations
        if self.enable_comment_warnings {
            if let Some(warning) = self.comment_checker.check_tool_result(tool_name, &input) {
                // Append comment warning to output
                let existing_text = output.as_text().unwrap_or("");
                let combined = if existing_text.is_empty() {
                    warning
                } else {
                    format!("{}\n\n---\n{}", existing_text, warning)
                };
                output = ToolOutput::text(combined);
            }
        }

        Ok(output)
    }

    /// Get the router for direct tool access (e.g., for approval checks)
    pub fn router(&self) -> &Arc<ToolRouter> {
        &self.router
    }

    async fn execute_with_sandbox(
        &self,
        tool: &BoxedTool,
        input: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        let sandbox = self
            .sandbox_manager
            .select_sandbox(tool.sandbox_preference());

        // First attempt with sandbox
        match self
            .execute_in_sandbox(tool, input.clone(), ctx, sandbox)
            .await
        {
            Ok(output) => Ok(output),
            Err(ToolError::ExecutionFailed { message }) if tool.escalate_on_failure() => {
                // Escalate: retry without sandbox (would need re-approval in real impl)
                tracing::warn!("Sandbox execution failed, escalating: {}", message);
                self.execute_without_sandbox(tool, input, ctx).await
            }
            err => err,
        }
    }

    async fn execute_in_sandbox(
        &self,
        tool: &BoxedTool,
        input: serde_json::Value,
        ctx: &ToolContext,
        _sandbox: SandboxType,
    ) -> Result<ToolOutput, ToolError> {
        // In a full implementation, we would wrap the execution in the sandbox
        // For now, just execute directly
        tool.execute(input, ctx).await
    }

    async fn execute_without_sandbox(
        &self,
        tool: &BoxedTool,
        input: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        tool.execute(input, ctx).await
    }

    async fn request_approval(
        &self,
        tool_name: &str,
        input: &serde_json::Value,
        reason: &str,
    ) -> Result<ReviewDecision, ToolError> {
        let id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = tokio::sync::oneshot::channel();

        let pending = PendingApproval {
            id: id.clone(),
            tool_name: tool_name.to_string(),
            input: input.clone(),
            reason: reason.to_string(),
            response_tx: tx,
        };

        self.approval_tx
            .send(pending)
            .await
            .map_err(|_| ToolError::ExecutionFailed {
                message: "Failed to send approval request".to_string(),
            })?;

        rx.await.map_err(|_| ToolError::ExecutionFailed {
            message: "Approval request cancelled".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FunctionTool;
    use uira_protocol::JsonSchema;

    fn create_test_router() -> Arc<ToolRouter> {
        let mut router = ToolRouter::new();
        router.register(FunctionTool::new(
            "safe_tool",
            "A safe tool",
            JsonSchema::object(),
            |_| async { Ok(ToolOutput::text("safe")) },
        ));
        Arc::new(router)
    }

    #[tokio::test]
    async fn test_orchestrator_run_safe_tool() {
        let router = create_test_router();
        let orchestrator =
            ToolOrchestrator::new(router, SandboxPolicy::full_access()).with_full_auto(true);
        let ctx = ToolContext::default();

        let result = orchestrator
            .run("safe_tool", serde_json::json!({}), &ctx)
            .await
            .unwrap();
        assert_eq!(result.as_text(), Some("safe"));
    }
}
