use async_trait::async_trait;
use once_cell::sync::OnceCell;
use std::sync::Arc;

use crate::hooks::hook::{Hook, HookContext, HookResult};
use crate::hooks::types::{HookEvent, HookInput, HookOutput};
use uira_memory::MemorySystem;

static MEMORY_SYSTEM: OnceCell<Arc<MemorySystem>> = OnceCell::new();

pub fn init_memory_system(system: Arc<MemorySystem>) -> bool {
    MEMORY_SYSTEM.set(system).is_ok()
}

pub fn get_memory_system() -> Option<&'static Arc<MemorySystem>> {
    MEMORY_SYSTEM.get()
}

pub struct MemoryRecallAdapter;

impl Default for MemoryRecallAdapter {
    fn default() -> Self {
        Self
    }
}

impl MemoryRecallAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Hook for MemoryRecallAdapter {
    fn name(&self) -> &str {
        "memory-recall"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::UserPromptSubmit]
    }

    async fn execute(
        &self,
        _event: HookEvent,
        input: &HookInput,
        _context: &HookContext,
    ) -> HookResult {
        let system = match get_memory_system() {
            Some(s) => s,
            None => return Ok(HookOutput::pass()),
        };

        let prompt = input.get_prompt_text();
        if prompt.is_empty() {
            return Ok(HookOutput::pass());
        }

        match system.recall_hook.recall(&prompt).await {
            Ok(Some(context)) => Ok(HookOutput::continue_with_message(context)),
            Ok(None) => Ok(HookOutput::pass()),
            Err(e) => {
                tracing::warn!(error = %e, "memory recall failed");
                Ok(HookOutput::pass())
            }
        }
    }

    fn priority(&self) -> i32 {
        50
    }
}

pub struct MemoryCaptureAdapter;

impl Default for MemoryCaptureAdapter {
    fn default() -> Self {
        Self
    }
}

impl MemoryCaptureAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Hook for MemoryCaptureAdapter {
    fn name(&self) -> &str {
        "memory-capture"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::Stop]
    }

    async fn execute(
        &self,
        _event: HookEvent,
        input: &HookInput,
        _context: &HookContext,
    ) -> HookResult {
        let system = match get_memory_system() {
            Some(s) => s,
            None => return Ok(HookOutput::pass()),
        };

        let prompt = input.get_prompt_text();
        let assistant_response = input.get_last_assistant_response().unwrap_or_default();

        if prompt.is_empty() && assistant_response.is_empty() {
            return Ok(HookOutput::pass());
        }

        let session_id = input.session_id.as_deref();

        match system
            .capture_hook
            .capture(&prompt, &assistant_response, session_id)
            .await
        {
            Ok(count) => {
                if count > 0 {
                    tracing::debug!(count, "memories captured");
                }
                Ok(HookOutput::pass())
            }
            Err(e) => {
                tracing::warn!(error = %e, "memory capture failed");
                Ok(HookOutput::pass())
            }
        }
    }

    fn priority(&self) -> i32 {
        -50
    }
}
