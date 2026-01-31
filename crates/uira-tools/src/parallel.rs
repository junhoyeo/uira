//! Parallel tool execution runtime with RwLock pattern

use futures::future::join_all;
use std::sync::Arc;
use tokio::sync::RwLock;
use uira_protocol::ToolOutput;

use crate::{ToolContext, ToolError, ToolRouter};

/// Runtime for executing tool calls with parallelism control
///
/// Uses a RwLock pattern:
/// - Parallel-safe tools acquire a read lock (multiple concurrent)
/// - Mutating tools acquire a write lock (exclusive)
pub struct ToolCallRuntime {
    router: Arc<ToolRouter>,
    parallel_lock: Arc<RwLock<()>>,
}

impl ToolCallRuntime {
    pub fn new(router: Arc<ToolRouter>) -> Self {
        Self {
            router,
            parallel_lock: Arc::new(RwLock::new(())),
        }
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
            // Parallel tools: read lock (multiple concurrent)
            let _guard = self.parallel_lock.read().await;
            self.router.dispatch(tool_name, input, ctx).await
        } else {
            // Mutating tools: write lock (exclusive)
            let _guard = self.parallel_lock.write().await;
            self.router.dispatch(tool_name, input, ctx).await
        }
    }

    /// Execute multiple tool calls, respecting parallelism
    pub async fn execute_batch(
        &self,
        calls: Vec<(String, serde_json::Value)>,
        ctx: &ToolContext,
    ) -> Vec<Result<ToolOutput, ToolError>> {
        // Partition into parallel and sequential
        let (parallel, sequential): (Vec<_>, Vec<_>) = calls
            .into_iter()
            .partition(|(name, _)| self.router.tool_supports_parallel(name));

        let mut results = Vec::new();

        // Execute parallel tools concurrently
        if !parallel.is_empty() {
            let _guard = self.parallel_lock.read().await;
            let handles: Vec<_> = parallel
                .into_iter()
                .map(|(name, input)| {
                    let router = self.router.clone();
                    let ctx = ToolContext {
                        cwd: ctx.cwd.clone(),
                        session_id: ctx.session_id.clone(),
                        full_auto: ctx.full_auto,
                        env: ctx.env.clone(),
                    };
                    tokio::spawn(async move { router.dispatch(&name, input, &ctx).await })
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

        // Execute sequential tools one at a time
        for (name, input) in sequential {
            let _guard = self.parallel_lock.write().await;
            results.push(self.router.dispatch(&name, input, ctx).await);
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
        // Partition into parallel and sequential while preserving IDs
        let (parallel, sequential): (Vec<_>, Vec<_>) = calls
            .into_iter()
            .enumerate()
            .partition(|(_, (_, name, _))| self.router.tool_supports_parallel(name));

        let total_count = parallel.len() + sequential.len();
        let mut indexed_results: Vec<(usize, String, Result<ToolOutput, ToolError>)> =
            Vec::with_capacity(total_count);

        // Execute parallel tools: spawn for true parallelism, join_all for concurrent collection
        if !parallel.is_empty() {
            let _guard = self.parallel_lock.read().await;
            let (metadata, handles): (Vec<_>, Vec<_>) = parallel
                .into_iter()
                .map(|(idx, (id, name, input))| {
                    let router = self.router.clone();
                    let ctx = ToolContext {
                        cwd: ctx.cwd.clone(),
                        session_id: ctx.session_id.clone(),
                        full_auto: ctx.full_auto,
                        env: ctx.env.clone(),
                    };
                    let handle =
                        tokio::spawn(async move { router.dispatch(&name, input, &ctx).await });
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

        // Execute sequential tools one at a time
        for (idx, (id, name, input)) in sequential {
            let _guard = self.parallel_lock.write().await;
            let result = self.router.dispatch(&name, input, ctx).await;
            indexed_results.push((idx, id, result));
        }

        // Sort by original index to preserve order
        indexed_results.sort_by_key(|(idx, _, _)| *idx);

        // Return just (id, result)
        indexed_results
            .into_iter()
            .map(|(_, id, result)| (id, result))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FunctionTool;
    use serde_json::json;
    use uira_protocol::JsonSchema;

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
