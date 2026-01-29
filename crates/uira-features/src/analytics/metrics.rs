//! Metrics collection and aggregation

use super::types::{MetricEvent, MetricQuery};
use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use tokio::fs::{create_dir_all, OpenOptions};
use tokio::io::AsyncWriteExt;

/// Metrics collector for recording and querying events
pub struct MetricsCollector {
    log_path: PathBuf,
}

impl MetricsCollector {
    /// Create a new metrics collector
    ///
    /// # Arguments
    /// * `base_dir` - Base directory for storing metrics (e.g., `.uira`)
    pub fn new(base_dir: impl AsRef<Path>) -> Self {
        let log_path = base_dir.as_ref().join("logs").join("metrics.jsonl");
        Self { log_path }
    }

    /// Ensure the log directory exists
    async fn ensure_log_dir(&self) -> Result<()> {
        if let Some(parent) = self.log_path.parent() {
            create_dir_all(parent)
                .await
                .context("Failed to create log directory")?;
        }
        Ok(())
    }

    /// Record a new metric event
    ///
    /// # Arguments
    /// * `event_type` - Type of event (e.g., "task_completed", "token_usage")
    /// * `data` - Event-specific data payload
    /// * `session_id` - Optional session identifier
    pub async fn record_event(
        &self,
        event_type: &str,
        data: Value,
        session_id: Option<&str>,
    ) -> Result<()> {
        self.ensure_log_dir().await?;

        let event = MetricEvent::new(event_type, data, session_id);
        let mut line = serde_json::to_string(&event).context("Failed to serialize metric event")?;
        line.push('\n');

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)
            .await
            .context("Failed to open metrics log file")?;

        file.write_all(line.as_bytes())
            .await
            .context("Failed to write metric event")?;

        Ok(())
    }

    /// Query metric events
    ///
    /// # Arguments
    /// * `query` - Query parameters for filtering events
    ///
    /// # Returns
    /// Vector of matching metric events
    pub async fn query(&self, query: MetricQuery) -> Result<Vec<MetricEvent>> {
        if !self.log_path.exists() {
            return Ok(Vec::new());
        }

        let file =
            std::fs::File::open(&self.log_path).context("Failed to open metrics log file")?;
        let reader = BufReader::new(file);

        let mut events = Vec::new();
        let mut skipped = 0;

        for line in reader.lines() {
            let line = line.context("Failed to read line from metrics log")?;
            if line.trim().is_empty() {
                continue;
            }

            let event: MetricEvent =
                serde_json::from_str(&line).context("Failed to parse metric event")?;

            if query.matches(&event) {
                // Handle offset
                if let Some(offset) = query.offset {
                    if skipped < offset {
                        skipped += 1;
                        continue;
                    }
                }

                events.push(event);

                // Handle limit
                if let Some(limit) = query.limit {
                    if events.len() >= limit {
                        break;
                    }
                }
            }
        }

        Ok(events)
    }

    /// Aggregate metric events using a custom aggregator function
    ///
    /// # Arguments
    /// * `query` - Query parameters for filtering events
    /// * `aggregator` - Function to aggregate the filtered events
    ///
    /// # Returns
    /// Aggregated result as JSON value
    pub async fn aggregate<F>(&self, query: MetricQuery, aggregator: F) -> Result<Value>
    where
        F: FnOnce(&[MetricEvent]) -> Value,
    {
        let events = self.query(query).await?;
        Ok(aggregator(&events))
    }
}

/// Common aggregation functions
pub mod aggregators {
    use super::*;

    /// Sum a numeric field across all events
    pub fn sum(field: &str) -> impl Fn(&[MetricEvent]) -> Value {
        let field = field.to_string();
        move |events: &[MetricEvent]| {
            let total: f64 = events
                .iter()
                .filter_map(|e| e.get_numeric_field(&field))
                .sum();
            serde_json::json!({ "sum": total })
        }
    }

    /// Calculate average of a numeric field
    pub fn avg(field: &str) -> impl Fn(&[MetricEvent]) -> Value {
        let field = field.to_string();
        move |events: &[MetricEvent]| {
            let values: Vec<f64> = events
                .iter()
                .filter_map(|e| e.get_numeric_field(&field))
                .collect();

            if values.is_empty() {
                return serde_json::json!({ "avg": 0.0, "count": 0 });
            }

            let total: f64 = values.iter().sum();
            let avg = total / values.len() as f64;

            serde_json::json!({ "avg": avg, "count": values.len() })
        }
    }

    /// Count the number of events
    pub fn count() -> impl Fn(&[MetricEvent]) -> Value {
        |events: &[MetricEvent]| serde_json::json!({ "count": events.len() })
    }

    /// Group events by a field value
    pub fn group_by(field: &str) -> impl Fn(&[MetricEvent]) -> Value {
        let field = field.to_string();
        move |events: &[MetricEvent]| {
            let mut groups: HashMap<String, Vec<&MetricEvent>> = HashMap::new();

            for event in events {
                let key = event
                    .get_string_field(&field)
                    .unwrap_or("unknown")
                    .to_string();
                groups.entry(key).or_default().push(event);
            }

            let result: HashMap<String, usize> = groups
                .into_iter()
                .map(|(key, events)| (key, events.len()))
                .collect();

            serde_json::to_value(result).unwrap_or(Value::Null)
        }
    }

    /// Calculate min, max, and average of a numeric field
    pub fn stats(field: &str) -> impl Fn(&[MetricEvent]) -> Value {
        let field = field.to_string();
        move |events: &[MetricEvent]| {
            let values: Vec<f64> = events
                .iter()
                .filter_map(|e| e.get_numeric_field(&field))
                .collect();

            if values.is_empty() {
                return serde_json::json!({
                    "count": 0,
                    "min": null,
                    "max": null,
                    "avg": null,
                    "sum": 0.0
                });
            }

            let min = values.iter().fold(f64::INFINITY, |a, &b| a.min(b));
            let max = values.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
            let sum: f64 = values.iter().sum();
            let avg = sum / values.len() as f64;

            serde_json::json!({
                "count": values.len(),
                "min": min,
                "max": max,
                "avg": avg,
                "sum": sum
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_record_and_query() {
        let temp_dir = TempDir::new().unwrap();
        let collector = MetricsCollector::new(temp_dir.path());

        // Record some events
        collector
            .record_event(
                "test_event",
                serde_json::json!({ "value": 42 }),
                Some("session1"),
            )
            .await
            .unwrap();

        collector
            .record_event(
                "test_event",
                serde_json::json!({ "value": 100 }),
                Some("session1"),
            )
            .await
            .unwrap();

        collector
            .record_event(
                "other_event",
                serde_json::json!({ "value": 200 }),
                Some("session2"),
            )
            .await
            .unwrap();

        // Query all test_event events
        let query = MetricQuery::new().with_event_type("test_event");
        let events = collector.query(query).await.unwrap();
        assert_eq!(events.len(), 2);

        // Query by session
        let query = MetricQuery::new().with_session_id("session2");
        let events = collector.query(query).await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].get_numeric_field("value"), Some(200.0));
    }

    #[tokio::test]
    async fn test_aggregators() {
        let temp_dir = TempDir::new().unwrap();
        let collector = MetricsCollector::new(temp_dir.path());

        // Record events with numeric values
        for i in 1..=5 {
            collector
                .record_event("metric", serde_json::json!({ "value": i * 10 }), None)
                .await
                .unwrap();
        }

        // Test sum
        let query = MetricQuery::new();
        let result = collector
            .aggregate(query.clone(), aggregators::sum("value"))
            .await
            .unwrap();
        assert_eq!(result["sum"], 150.0);

        // Test avg
        let result = collector
            .aggregate(query.clone(), aggregators::avg("value"))
            .await
            .unwrap();
        assert_eq!(result["avg"], 30.0);
        assert_eq!(result["count"], 5);

        // Test count
        let result = collector
            .aggregate(query, aggregators::count())
            .await
            .unwrap();
        assert_eq!(result["count"], 5);
    }
}
