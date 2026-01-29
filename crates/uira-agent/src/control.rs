//! Agent control and spawn limits

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

/// Guards for spawn limits and concurrent execution
pub struct Guards {
    /// Maximum concurrent tool executions
    max_concurrent_tools: usize,
    /// Current count of running tools
    running_tools: AtomicUsize,
    /// Maximum spawned sub-agents
    max_spawned_agents: usize,
    /// Current count of spawned agents
    spawned_agents: AtomicUsize,
}

impl Guards {
    pub fn new(max_concurrent_tools: usize, max_spawned_agents: usize) -> Self {
        Self {
            max_concurrent_tools,
            running_tools: AtomicUsize::new(0),
            max_spawned_agents,
            spawned_agents: AtomicUsize::new(0),
        }
    }

    /// Try to acquire a tool execution slot
    pub fn try_acquire_tool_slot(&self) -> Option<ToolGuard<'_>> {
        let current = self.running_tools.fetch_add(1, Ordering::SeqCst);
        if current >= self.max_concurrent_tools {
            self.running_tools.fetch_sub(1, Ordering::SeqCst);
            None
        } else {
            Some(ToolGuard { guards: self })
        }
    }

    /// Try to acquire an agent spawn slot
    pub fn try_acquire_agent_slot(&self) -> Option<AgentGuard<'_>> {
        let current = self.spawned_agents.fetch_add(1, Ordering::SeqCst);
        if current >= self.max_spawned_agents {
            self.spawned_agents.fetch_sub(1, Ordering::SeqCst);
            None
        } else {
            Some(AgentGuard { guards: self })
        }
    }

    pub fn running_tools(&self) -> usize {
        self.running_tools.load(Ordering::SeqCst)
    }

    pub fn spawned_agents(&self) -> usize {
        self.spawned_agents.load(Ordering::SeqCst)
    }
}

impl Default for Guards {
    fn default() -> Self {
        Self::new(10, 5)
    }
}

/// RAII guard for tool execution slot
pub struct ToolGuard<'a> {
    guards: &'a Guards,
}

impl Drop for ToolGuard<'_> {
    fn drop(&mut self) {
        self.guards.running_tools.fetch_sub(1, Ordering::SeqCst);
    }
}

/// RAII guard for spawned agent slot
pub struct AgentGuard<'a> {
    guards: &'a Guards,
}

impl Drop for AgentGuard<'_> {
    fn drop(&mut self) {
        self.guards.spawned_agents.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Agent control handle for managing the agent externally
pub struct AgentControl {
    cancelled: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    guards: Arc<Guards>,
}

impl AgentControl {
    pub fn new(guards: Arc<Guards>) -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            paused: Arc::new(AtomicBool::new(false)),
            guards,
        }
    }

    /// Cancel the agent
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    /// Pause the agent
    pub fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
    }

    /// Resume the agent
    pub fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
    }

    /// Check if cancelled
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// Check if paused
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    /// Get the guards
    pub fn guards(&self) -> &Arc<Guards> {
        &self.guards
    }

    /// Create a clone of the cancel signal for sharing
    pub fn cancel_signal(&self) -> Arc<AtomicBool> {
        self.cancelled.clone()
    }
}

impl Default for AgentControl {
    fn default() -> Self {
        Self::new(Arc::new(Guards::default()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_guards_tool_slots() {
        let guards = Guards::new(2, 5);

        let _guard1 = guards.try_acquire_tool_slot().unwrap();
        let _guard2 = guards.try_acquire_tool_slot().unwrap();
        assert!(guards.try_acquire_tool_slot().is_none());

        assert_eq!(guards.running_tools(), 2);
    }

    #[test]
    fn test_agent_control() {
        let control = AgentControl::default();

        assert!(!control.is_cancelled());
        assert!(!control.is_paused());

        control.cancel();
        assert!(control.is_cancelled());

        control.pause();
        assert!(control.is_paused());

        control.resume();
        assert!(!control.is_paused());
    }
}
