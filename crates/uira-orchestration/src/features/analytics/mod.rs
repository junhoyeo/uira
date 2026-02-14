//! Analytics and metrics collection for Uira
//!
//! This module provides:
//! - Token tracking and usage metrics
//! - Cost estimation for model usage
//! - Session metrics collection and aggregation
//! - JSONL-based event logging

pub mod cost;
pub mod metrics;
pub mod types;

pub use cost::CostEstimator;
pub use metrics::{aggregators, MetricsCollector};
pub use types::{MetricEvent, MetricQuery};
