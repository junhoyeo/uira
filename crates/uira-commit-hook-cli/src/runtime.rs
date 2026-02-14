// crates/uira/src/runtime.rs

use std::sync::OnceLock;
use tokio::runtime::Runtime;

/// Shared Tokio runtime for agent workflows.
/// Uses current-thread runtime to avoid nested runtime issues.
static RUNTIME: OnceLock<Runtime> = OnceLock::new();

/// Get the shared Tokio runtime.
/// Creates it on first access (lazy initialization).
pub fn get_runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create Tokio runtime")
    })
}

/// Run an async function on the shared runtime.
pub fn block_on<F: std::future::Future>(f: F) -> F::Output {
    get_runtime().block_on(f)
}
