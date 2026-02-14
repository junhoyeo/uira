//! Tool orchestrator for permission → approval → sandbox → escalate flow

use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use uira_permissions::{Action as PermissionAction, PermissionEvaluator};
use uira_types::{ApprovalRequirement, ReviewDecision, ToolOutput};
use uira_sandbox::{SandboxManager, SandboxPolicy, SandboxType};

use crate::tools::approval_cache::{ApprovalCache, ApprovalKey, CacheDecision};
use crate::tools::comment_hook::CommentChecker;
use crate::tools::{BoxedTool, ToolContext, ToolError, ToolRouter};

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

/// Orchestrator for tool execution with permission, approval and sandboxing
pub struct ToolOrchestrator {
    router: Arc<ToolRouter>,
    sandbox_manager: SandboxManager,
    comment_checker: CommentChecker,
    permission_evaluator: Option<PermissionEvaluator>,
    approval_cache: Option<Arc<RwLock<ApprovalCache>>>,
    approval_tx: mpsc::Sender<PendingApproval>,
    approval_rx: Option<mpsc::Receiver<PendingApproval>>,
    full_auto: bool,
    enable_comment_warnings: bool,
}

impl ToolOrchestrator {
    fn provider_approval_requirement(
        tool_name: &str,
        input: &serde_json::Value,
    ) -> ApprovalRequirement {
        match tool_name {
            "ast_replace" => {
                let dry_run = input
                    .get("dryRun")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                if dry_run {
                    ApprovalRequirement::skip()
                } else {
                    ApprovalRequirement::needs_approval(
                        "ast_replace with dryRun=false writes files and requires explicit approval",
                    )
                }
            }
            "lsp_rename" => ApprovalRequirement::needs_approval(
                "lsp_rename can modify files across the workspace",
            ),
            _ => ApprovalRequirement::skip(),
        }
    }

    pub fn new(router: Arc<ToolRouter>, sandbox_policy: SandboxPolicy) -> Self {
        let (tx, rx) = mpsc::channel(100);
        Self {
            router,
            sandbox_manager: SandboxManager::new(sandbox_policy),
            comment_checker: CommentChecker::new(),
            permission_evaluator: None,
            approval_cache: None,
            approval_tx: tx,
            approval_rx: Some(rx),
            full_auto: false,
            enable_comment_warnings: true,
        }
    }

    pub fn with_permission_evaluator(mut self, evaluator: PermissionEvaluator) -> Self {
        self.permission_evaluator = Some(evaluator);
        self
    }

    pub fn with_approval_cache(mut self, cache: ApprovalCache) -> Self {
        self.approval_cache = Some(Arc::new(RwLock::new(cache)));
        self
    }

    pub fn approval_cache(&self) -> Option<Arc<RwLock<ApprovalCache>>> {
        self.approval_cache.clone()
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

        // Provider-backed tools still need permission and approval checks.
        // They are dispatched via providers after the same top-level gates used for direct tools.
        if direct_tool.is_none() {
            let mut provider_input = input;

            if let Some(ref evaluator) = self.permission_evaluator {
                let perm_result = evaluator.evaluate_tool(tool_name, &provider_input);

                tracing::debug!(
                    permission = %perm_result.permission,
                    path = %perm_result.path,
                    action = ?perm_result.action,
                    rule = ?perm_result.matched_rule,
                    "permission_evaluated"
                );

                match perm_result.action {
                    PermissionAction::Deny => {
                        let rule_info = perm_result
                            .matched_rule
                            .map(|r| format!(" (rule: {})", r))
                            .unwrap_or_default();
                        tracing::warn!(
                            tool = %tool_name,
                            permission = %perm_result.permission,
                            path = %perm_result.path,
                            "permission_denied"
                        );
                        return Err(ToolError::PermissionDenied {
                            message: format!(
                                "Permission denied for {} on {}{}",
                                perm_result.permission, perm_result.path, rule_info
                            ),
                        });
                    }
                    PermissionAction::Allow => {
                        // Continue directly to provider dispatch after permission allow.
                    }
                    PermissionAction::Ask => {}
                }
            }

            if !options.skip_approval {
                let requirement = self.approval_requirement_for(tool_name, &provider_input);
                match requirement {
                    ApprovalRequirement::Skip { .. } => {}
                    ApprovalRequirement::NeedsApproval { reason } => {
                        if !self.full_auto && !ctx.full_auto {
                            let path = Self::extract_path_from_input(&provider_input);

                            if let Some(ref cache) = self.approval_cache {
                                let cache_read = cache.read().await;
                                if let Some(cached) = cache_read.lookup(tool_name, &path) {
                                    tracing::debug!(
                                        tool = %tool_name,
                                        path = %path,
                                        decision = ?cached,
                                        "approval_cache_hit"
                                    );
                                    if !cached.is_approve() {
                                        return Err(ToolError::ExecutionFailed {
                                            message: "Approval denied (cached)".to_string(),
                                        });
                                    }
                                } else {
                                    let decision = self
                                        .request_approval(tool_name, &provider_input, &reason)
                                        .await?;
                                    let cache_decision = Self::review_to_cache_decision(&decision);
                                    if cache_decision.should_cache() {
                                        let key = ApprovalKey::from_tool_and_path(tool_name, &path);
                                        let mut cache_write = cache.write().await;
                                        cache_write.insert(key, cache_decision);
                                    }

                                    if decision.is_denied() {
                                        return Err(ToolError::ExecutionFailed {
                                            message: "Approval denied by user".to_string(),
                                        });
                                    }

                                    if let ReviewDecision::Edit { new_input } = decision {
                                        provider_input = new_input;
                                    }
                                }
                            } else {
                                let decision = self
                                    .request_approval(tool_name, &provider_input, &reason)
                                    .await?;
                                if decision.is_denied() {
                                    return Err(ToolError::ExecutionFailed {
                                        message: "Approval denied by user".to_string(),
                                    });
                                }
                                if let ReviewDecision::Edit { new_input } = decision {
                                    provider_input = new_input;
                                }
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

            return self.router.dispatch(tool_name, provider_input, ctx).await;
        }

        let tool = direct_tool.unwrap();

        // 0. Evaluate permission rules (if evaluator is configured)
        if let Some(ref evaluator) = self.permission_evaluator {
            let perm_result = evaluator.evaluate_tool(tool_name, &input);

            tracing::debug!(
                permission = %perm_result.permission,
                path = %perm_result.path,
                action = ?perm_result.action,
                rule = ?perm_result.matched_rule,
                "permission_evaluated"
            );

            match perm_result.action {
                PermissionAction::Deny => {
                    let rule_info = perm_result
                        .matched_rule
                        .map(|r| format!(" (rule: {})", r))
                        .unwrap_or_default();
                    tracing::warn!(
                        tool = %tool_name,
                        permission = %perm_result.permission,
                        path = %perm_result.path,
                        "permission_denied"
                    );
                    return Err(ToolError::PermissionDenied {
                        message: format!(
                            "Permission denied for {} on {}{}",
                            perm_result.permission, perm_result.path, rule_info
                        ),
                    });
                }
                PermissionAction::Allow => {
                    // Permission explicitly allowed - skip approval flow
                    // (unless tool itself has a Forbidden requirement)
                    let requirement = tool.approval_requirement(&input);
                    if matches!(requirement, ApprovalRequirement::Forbidden { .. }) {
                        if let ApprovalRequirement::Forbidden { reason } = requirement {
                            return Err(ToolError::ExecutionFailed {
                                message: format!("Tool execution forbidden: {}", reason),
                            });
                        }
                    }
                    return if options.skip_sandbox {
                        self.execute_without_sandbox(tool, input, ctx).await
                    } else {
                        self.execute_with_sandbox(tool, input, ctx).await
                    };
                }
                PermissionAction::Ask => {
                    // Fall through to approval flow
                }
            }
        }

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
                        let path = Self::extract_path_from_input(&input);

                        if let Some(ref cache) = self.approval_cache {
                            let cache_read = cache.read().await;
                            if let Some(cached) = cache_read.lookup(tool_name, &path) {
                                tracing::debug!(
                                    tool = %tool_name,
                                    path = %path,
                                    decision = ?cached,
                                    "approval_cache_hit"
                                );
                                if cached.is_approve() {
                                    return self.execute_with_sandbox(tool, input, ctx).await;
                                } else {
                                    return Err(ToolError::ExecutionFailed {
                                        message: "Approval denied (cached)".to_string(),
                                    });
                                }
                            }
                        }

                        let decision = self.request_approval(tool_name, &input, &reason).await?;

                        if let Some(ref cache) = self.approval_cache {
                            let cache_decision = Self::review_to_cache_decision(&decision);
                            if cache_decision.should_cache() {
                                let key = ApprovalKey::from_tool_and_path(tool_name, &path);
                                let mut cache_write = cache.write().await;
                                cache_write.insert(key, cache_decision);
                                tracing::debug!(
                                    tool = %tool_name,
                                    path = %path,
                                    decision = ?cache_decision,
                                    "approval_cached"
                                );
                            }
                        }

                        if decision.is_denied() {
                            return Err(ToolError::ExecutionFailed {
                                message: "Approval denied by user".to_string(),
                            });
                        }
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

    /// Determine approval requirement for any tool name (direct or provider-backed).
    pub fn approval_requirement_for(
        &self,
        tool_name: &str,
        input: &serde_json::Value,
    ) -> ApprovalRequirement {
        if let Some(tool) = self.router.get(tool_name) {
            tool.approval_requirement(input)
        } else {
            Self::provider_approval_requirement(tool_name, input)
        }
    }

    async fn execute_with_sandbox(
        &self,
        tool: &BoxedTool,
        input: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        self.execute_with_retry(tool, input, ctx, 0).await
    }

    fn execute_with_retry<'a>(
        &'a self,
        tool: &'a BoxedTool,
        input: serde_json::Value,
        ctx: &'a ToolContext,
        attempt: u32,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<ToolOutput, ToolError>> + Send + 'a>,
    > {
        Box::pin(async move {
            const MAX_ATTEMPTS: u32 = 2;

            let sandbox = self
                .sandbox_manager
                .select_sandbox(tool.sandbox_preference());

            match self
                .execute_in_sandbox(tool, input.clone(), ctx, sandbox)
                .await
            {
                Ok(output) => Ok(output),
                Err(ToolError::SandboxDenied { message, retryable })
                    if retryable && attempt < MAX_ATTEMPTS - 1 =>
                {
                    tracing::warn!(
                        tool = %tool.name(),
                        attempt = attempt + 1,
                        max_attempts = MAX_ATTEMPTS,
                        reason = %message,
                        "tool_retried"
                    );
                    self.execute_with_retry(tool, input, ctx, attempt + 1).await
                }
                Err(ToolError::SandboxDenied { message, .. }) => {
                    Err(ToolError::sandbox_denied_final(format!(
                        "Sandbox denied after {} attempts: {}",
                        attempt + 1,
                        message
                    )))
                }
                Err(ToolError::ExecutionFailed { message }) if tool.escalate_on_failure() => {
                    tracing::warn!("Sandbox execution failed, escalating: {}", message);
                    self.execute_without_sandbox(tool, input, ctx).await
                }
                err => err,
            }
        })
    }

    async fn execute_in_sandbox(
        &self,
        tool: &BoxedTool,
        input: serde_json::Value,
        ctx: &ToolContext,
        sandbox: SandboxType,
    ) -> Result<ToolOutput, ToolError> {
        let sandboxed_ctx = ToolContext {
            cwd: ctx.cwd.clone(),
            session_id: ctx.session_id.clone(),
            full_auto: ctx.full_auto,
            env: ctx.env.clone(),
            sandbox_type: sandbox,
            sandbox_policy: ctx.sandbox_policy.clone(),
        };
        tool.execute(input, &sandboxed_ctx).await
    }

    async fn execute_without_sandbox(
        &self,
        tool: &BoxedTool,
        input: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        tool.execute(input, ctx).await
    }

    fn extract_path_from_input(input: &serde_json::Value) -> String {
        // Path field priority order - must match uira_permissions::evaluator::extract_path_from_input
        let path_fields = [
            "path",
            "file_path",
            "filePath",
            "file",
            "url",
            "uri",
            "query",
            "target",
            "directory",
            "dir",
        ];

        for field in path_fields {
            if let Some(path) = input.get(field).and_then(|v| v.as_str()) {
                return path.to_string();
            }
        }

        // For bash/shell commands, use the command itself
        if let Some(command) = input.get("command").and_then(|v| v.as_str()) {
            return command.to_string();
        }

        // Fallback: wildcard for cache matching
        "*".to_string()
    }

    fn review_to_cache_decision(decision: &ReviewDecision) -> CacheDecision {
        match decision {
            ReviewDecision::Approve => CacheDecision::ApproveForSession,
            ReviewDecision::ApproveOnce => CacheDecision::ApproveOnce,
            ReviewDecision::ApproveAll => CacheDecision::ApproveForPattern,
            ReviewDecision::Deny { .. } => CacheDecision::DenyForSession,
            ReviewDecision::Edit { .. } => CacheDecision::ApproveOnce,
        }
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

    /// Evaluate permission rules for a tool call (for Agent-level integration)
    ///
    /// Returns None if no permission evaluator is configured, otherwise returns
    /// the permission action (Allow, Deny, or Ask)
    pub fn evaluate_permission(
        &self,
        tool_name: &str,
        input: &serde_json::Value,
    ) -> Option<PermissionAction> {
        self.permission_evaluator.as_ref().map(|evaluator| {
            let result = evaluator.evaluate_tool(tool_name, input);
            tracing::debug!(
                tool = %tool_name,
                permission = %result.permission,
                path = %result.path,
                action = ?result.action,
                "agent_permission_evaluated"
            );
            result.action
        })
    }

    /// Check approval cache for a prior decision (for Agent-level integration)
    ///
    /// Returns the cached decision if one exists and is still valid
    pub async fn check_approval_cache(
        &self,
        tool_name: &str,
        input: &serde_json::Value,
    ) -> Option<CacheDecision> {
        let cache = self.approval_cache.as_ref()?;
        let guard = cache.read().await;

        if tool_name == "Bash" {
            let command = input.get("command").and_then(|v| v.as_str()).unwrap_or("");
            let working_dir = input
                .get("working_directory")
                .and_then(|v| v.as_str())
                .unwrap_or(".");
            guard.lookup_bash(command, working_dir)
        } else {
            let path = Self::extract_path_from_input(input);
            guard.lookup(tool_name, &path)
        }
    }

    /// Store an approval decision in the cache (for Agent-level integration)
    pub async fn store_approval(
        &self,
        tool_name: &str,
        input: &serde_json::Value,
        decision: &ReviewDecision,
    ) {
        if let Some(cache) = &self.approval_cache {
            let key = if tool_name == "Bash" {
                let command = input.get("command").and_then(|v| v.as_str()).unwrap_or("");
                let working_dir = input
                    .get("working_directory")
                    .and_then(|v| v.as_str())
                    .unwrap_or(".");
                ApprovalKey::for_bash_command(command, working_dir)
            } else {
                let path = Self::extract_path_from_input(input);
                ApprovalKey::new(tool_name, &path)
            };

            let cache_decision = Self::review_to_cache_decision(decision);

            let mut guard = cache.write().await;
            guard.insert(key.clone(), cache_decision);
            tracing::debug!(
                tool = %tool_name,
                pattern = %key.pattern,
                decision = ?cache_decision,
                "agent_approval_cached"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::FunctionTool;
    use uira_types::JsonSchema;

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
