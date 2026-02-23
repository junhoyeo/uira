//! Parallel tool execution runtime with RwLock pattern

use futures::future::join_all;
use std::sync::Arc;
use tokio::sync::RwLock;
use uira_core::ToolOutput;

use crate::tools::{ToolContext, ToolError, ToolOrchestrator, ToolRouter};

/// Runtime for executing tool calls with parallelism control
///
/// Uses a RwLock pattern:
/// - Parallel-safe tools acquire a read lock (multiple concurrent)
/// - Mutating tools acquire a write lock (exclusive)
///
/// When an orchestrator is configured, tool calls route through permission
/// and approval checks. Without an orchestrator, calls go directly to the router
/// (use only in trusted/test contexts).
pub struct ToolCallRuntime {
    router: Arc<ToolRouter>,
    orchestrator: Option<Arc<ToolOrchestrator>>,
    parallel_lock: Arc<RwLock<()>>,
}

impl ToolCallRuntime {
    pub fn new(router: Arc<ToolRouter>) -> Self {
        Self {
            router,
            orchestrator: None,
            parallel_lock: Arc::new(RwLock::new(())),
        }
    }

    /// Configure an orchestrator to route tool calls through permission/approval checks
    pub fn with_orchestrator(mut self, orchestrator: Arc<ToolOrchestrator>) -> Self {
        self.orchestrator = Some(orchestrator);
        self
    }

    /// Execute a tool call with proper parallelism control
    pub async fn execute(
        &self,
        tool_name: &str,
        input: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        let supports_parallel = self.router.tool_supports_parallel(tool_name);

        if supports_parallel {
            let _guard = self.parallel_lock.read().await;
            self.dispatch_tool(tool_name, input, ctx).await
        } else {
            let _guard = self.parallel_lock.write().await;
            self.dispatch_tool(tool_name, input, ctx).await
        }
    }

    async fn dispatch_tool(
        &self,
        tool_name: &str,
        input: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        if let Some(ref orchestrator) = self.orchestrator {
            orchestrator.run(tool_name, input, ctx).await
        } else {
            self.router.dispatch(tool_name, input, ctx).await
        }
    }

    /// Execute multiple tool calls, respecting parallelism
    pub async fn execute_batch(
        &self,
        calls: Vec<(String, serde_json::Value)>,
        ctx: &ToolContext,
    ) -> Vec<Result<ToolOutput, ToolError>> {
        let (parallel, sequential): (Vec<_>, Vec<_>) = calls
            .into_iter()
            .partition(|(name, _)| self.router.tool_supports_parallel(name));

        let mut results = Vec::new();

        if !parallel.is_empty() {
            let _guard = self.parallel_lock.read().await;
            let handles: Vec<_> = parallel
                .into_iter()
                .map(|(name, input)| {
                    let orchestrator = self.orchestrator.clone();
                    let router = self.router.clone();
                    let ctx = ToolContext {
                        cwd: ctx.cwd.clone(),
                        session_id: ctx.session_id.clone(),
                        memory_system: ctx.memory_system.clone(),
                        full_auto: ctx.full_auto,
                        env: ctx.env.clone(),
                        sandbox_type: ctx.sandbox_type,
                        sandbox_policy: ctx.sandbox_policy.clone(),
                    };
                    tokio::spawn(async move {
                        if let Some(ref orch) = orchestrator {
                            orch.run(&name, input, &ctx).await
                        } else {
                            router.dispatch(&name, input, &ctx).await
                        }
                    })
                })
                .collect();

            for handle in handles {
                results.push(handle.await.unwrap_or_else(|e| {
                    Err(ToolError::ExecutionFailed {
                        message: e.to_string(),
                    })
                }));
            }
        }

        for (name, input) in sequential {
            let _guard = self.parallel_lock.write().await;
            results.push(self.dispatch_tool(&name, input, ctx).await);
        }

        results
    }

    /// Get the router
    pub fn router(&self) -> &Arc<ToolRouter> {
        &self.router
    }

    /// Execute multiple tool calls with IDs, respecting parallelism
    /// Returns results in the same order as input calls
    pub async fn execute_batch_with_ids(
        &self,
        calls: Vec<(String, String, serde_json::Value)>, // (id, name, input)
        ctx: &ToolContext,
    ) -> Vec<(String, Result<ToolOutput, ToolError>)> {
        let (parallel, sequential): (Vec<_>, Vec<_>) = calls
            .into_iter()
            .enumerate()
            .partition(|(_, (_, name, _))| self.router.tool_supports_parallel(name));

        let total_count = parallel.len() + sequential.len();
        let mut indexed_results: Vec<(usize, String, Result<ToolOutput, ToolError>)> =
            Vec::with_capacity(total_count);

        if !parallel.is_empty() {
            let _guard = self.parallel_lock.read().await;
            let (metadata, handles): (Vec<_>, Vec<_>) = parallel
                .into_iter()
                .map(|(idx, (id, name, input))| {
                    let orchestrator = self.orchestrator.clone();
                    let router = self.router.clone();
                    let ctx = ToolContext {
                        cwd: ctx.cwd.clone(),
                        session_id: ctx.session_id.clone(),
                        memory_system: ctx.memory_system.clone(),
                        full_auto: ctx.full_auto,
                        env: ctx.env.clone(),
                        sandbox_type: ctx.sandbox_type,
                        sandbox_policy: ctx.sandbox_policy.clone(),
                    };
                    let handle = tokio::spawn(async move {
                        if let Some(ref orch) = orchestrator {
                            orch.run(&name, input, &ctx).await
                        } else {
                            router.dispatch(&name, input, &ctx).await
                        }
                    });
                    ((idx, id), handle)
                })
                .unzip();

            let join_results = join_all(handles).await;
            for ((idx, id), join_result) in metadata.into_iter().zip(join_results) {
                let result = join_result.unwrap_or_else(|e| {
                    Err(ToolError::ExecutionFailed {
                        message: format!("Task panicked: {}", e),
                    })
                });
                indexed_results.push((idx, id, result));
            }
        }

        for (idx, (id, name, input)) in sequential {
            let _guard = self.parallel_lock.write().await;
            let result = self.dispatch_tool(&name, input, ctx).await;
            indexed_results.push((idx, id, result));
        }

        indexed_results.sort_by_key(|(idx, _, _)| *idx);

        indexed_results
            .into_iter()
            .map(|(_, id, result)| (id, result))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::FunctionTool;
    use serde_json::json;
    use uira_core::JsonSchema;

    fn create_test_router() -> Arc<ToolRouter> {
        let mut router = ToolRouter::new();

        router.register(
            FunctionTool::new(
                "parallel_tool",
                "A parallel-safe tool",
                JsonSchema::object(),
                |_| async { Ok(ToolOutput::text("parallel")) },
            )
            .with_parallel(true),
        );

        router.register(
            FunctionTool::new(
                "sequential_tool",
                "A sequential tool",
                JsonSchema::object(),
                |_| async { Ok(ToolOutput::text("sequential")) },
            )
            .with_parallel(false),
        );

        Arc::new(router)
    }

    #[tokio::test]
    async fn test_parallel_execution() {
        let router = create_test_router();
        let runtime = ToolCallRuntime::new(router);
        let ctx = ToolContext::default();

        let result = runtime
            .execute("parallel_tool", json!({}), &ctx)
            .await
            .unwrap();
        assert_eq!(result.as_text(), Some("parallel"));
    }

    #[tokio::test]
    async fn test_batch_execution() {
        let router = create_test_router();
        let runtime = ToolCallRuntime::new(router);
        let ctx = ToolContext::default();

        let calls = vec![
            ("parallel_tool".to_string(), json!({})),
            ("sequential_tool".to_string(), json!({})),
        ];

        let results = runtime.execute_batch(calls, &ctx).await;
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.is_ok()));
    }

    #[tokio::test]
    async fn test_parallel_speedup() {
        use std::time::{Duration, Instant};

        let mut router = ToolRouter::new();
        router.register(
            FunctionTool::new(
                "slow_parallel",
                "Slow parallel tool",
                JsonSchema::object(),
                |_| async {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    Ok(ToolOutput::text("done"))
                },
            )
            .with_parallel(true),
        );

        let runtime = ToolCallRuntime::new(Arc::new(router));
        let ctx = ToolContext::default();

        let calls: Vec<_> = (0..5)
            .map(|i| (format!("id_{}", i), "slow_parallel".to_string(), json!({})))
            .collect();

        let start = Instant::now();
        let results = runtime.execute_batch_with_ids(calls, &ctx).await;
        let parallel_time = start.elapsed();

        assert_eq!(results.len(), 5);
        assert!(results.iter().all(|(_, r)| r.is_ok()));
        assert!(
            parallel_time < Duration::from_millis(150),
            "5 parallel 50ms tasks should complete in <150ms, took {:?}",
            parallel_time
        );
    }

    #[tokio::test]
    async fn test_batch_with_ids_preserves_order() {
        let router = create_test_router();
        let runtime = ToolCallRuntime::new(router);
        let ctx = ToolContext::default();

        let calls = vec![
            ("id_0".to_string(), "parallel_tool".to_string(), json!({})),
            ("id_1".to_string(), "sequential_tool".to_string(), json!({})),
            ("id_2".to_string(), "parallel_tool".to_string(), json!({})),
        ];

        let results = runtime.execute_batch_with_ids(calls, &ctx).await;

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].0, "id_0");
        assert_eq!(results[1].0, "id_1");
        assert_eq!(results[2].0, "id_2");
    }
}
