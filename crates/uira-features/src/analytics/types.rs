//! Core types for analytics and metrics

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A single metric event recorded during a session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricEvent {
    /// Timestamp when the event occurred
    pub timestamp: DateTime<Utc>,
    /// Type of event (e.g., "task_completed", "token_usage", "error")
    pub event_type: String,
    /// Event-specific data payload
    pub data: Value,
    /// Optional session identifier
    pub session_id: Option<String>,
}

impl MetricEvent {
    /// Create a new metric event
    pub fn new(event_type: impl Into<String>, data: Value, session_id: Option<&str>) -> Self {
        Self {
            timestamp: Utc::now(),
            event_type: event_type.into(),
            data,
            session_id: session_id.map(|s| s.to_string()),
        }
    }

    /// Get a field value from the data payload
    pub fn get_field(&self, field: &str) -> Option<&Value> {
        self.data.get(field)
    }

    /// Get a numeric field value
    pub fn get_numeric_field(&self, field: &str) -> Option<f64> {
        self.get_field(field)?.as_f64()
    }

    /// Get a string field value
    pub fn get_string_field(&self, field: &str) -> Option<&str> {
        self.get_field(field)?.as_str()
    }
}

/// Query parameters for filtering metric events
#[derive(Debug, Clone, Default)]
pub struct MetricQuery {
    /// Filter by event type
    pub event_type: Option<String>,
    /// Filter by start date (inclusive)
    pub start_date: Option<DateTime<Utc>>,
    /// Filter by end date (inclusive)
    pub end_date: Option<DateTime<Utc>>,
    /// Filter by session ID
    pub session_id: Option<String>,
    /// Maximum number of results
    pub limit: Option<usize>,
    /// Number of results to skip
    pub offset: Option<usize>,
}

impl MetricQuery {
    /// Create a new empty query
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter by event type
    pub fn with_event_type(mut self, event_type: impl Into<String>) -> Self {
        self.event_type = Some(event_type.into());
        self
    }

    /// Filter by date range
    pub fn with_date_range(mut self, start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        self.start_date = Some(start);
        self.end_date = Some(end);
        self
    }

    /// Filter by session ID
    pub fn with_session_id(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Limit number of results
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Skip first N results
    pub fn with_offset(mut self, offset: usize) -> Self {
        self.offset = Some(offset);
        self
    }

    /// Check if an event matches this query
    pub fn matches(&self, event: &MetricEvent) -> bool {
        // Check event type
        if let Some(ref event_type) = self.event_type {
            if &event.event_type != event_type {
                return false;
            }
        }

        // Check start date
        if let Some(start_date) = self.start_date {
            if event.timestamp < start_date {
                return false;
            }
        }

        // Check end date
        if let Some(end_date) = self.end_date {
            if event.timestamp > end_date {
                return false;
            }
        }

        // Check session ID
        if let Some(ref session_id) = self.session_id {
            match &event.session_id {
                Some(event_session_id) => {
                    if event_session_id != session_id {
                        return false;
                    }
                }
                None => return false,
            }
        }

        true
    }
}
