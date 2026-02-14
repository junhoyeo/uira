use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

pub struct ChannelLayer {
    tx: UnboundedSender<String>,
}

impl ChannelLayer {
    pub fn new(tx: UnboundedSender<String>) -> Self {
        Self { tx }
    }
}

#[derive(Default)]
struct MessageVisitor {
    message: Option<String>,
}

impl tracing::field::Visit for MessageVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = Some(format!("{:?}", value));
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_string());
        }
    }
}

impl<S> tracing_subscriber::Layer<S> for ChannelLayer
where
    S: Subscriber,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let metadata = event.metadata();
        if !matches!(*metadata.level(), Level::WARN | Level::ERROR) {
            return;
        }

        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);
        let message = visitor
            .message
            .unwrap_or_else(|| "(no message)".to_string());
        let formatted = format!("{} {}: {}", metadata.level(), metadata.target(), message);

        let _ = self.tx.send(formatted);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryConfig {
    #[serde(default = "default_level")]
    pub level: String,

    #[serde(default)]
    pub json_output: bool,

    #[serde(default)]
    pub otlp_endpoint: Option<String>,

    #[serde(default)]
    pub service_name: Option<String>,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            level: default_level(),
            json_output: false,
            otlp_endpoint: None,
            service_name: None,
        }
    }
}

fn default_level() -> String {
    "info".to_string()
}

pub fn init_subscriber(config: &TelemetryConfig) {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.level));

    if config.json_output {
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt::layer().json())
            .init();
    } else {
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt::layer())
            .init();
    }
}

pub fn init_tui_subscriber(config: &TelemetryConfig) -> UnboundedReceiver<String> {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.level));
    let (tx, rx) = unbounded_channel();

    tracing_subscriber::registry()
        .with(filter)
        .with(ChannelLayer::new(tx))
        .init();

    rx
}

#[cfg(feature = "otlp")]
pub fn init_otlp_subscriber(config: &TelemetryConfig) -> Result<(), Box<dyn std::error::Error>> {
    use opentelemetry::global;
    use opentelemetry_otlp::WithExportConfig;
    use opentelemetry_sdk::{runtime, trace as sdktrace};

    let endpoint = config
        .otlp_endpoint
        .as_deref()
        .unwrap_or("http://localhost:4317");

    let service_name = config.service_name.as_deref().unwrap_or("uira-agent");

    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint(endpoint),
        )
        .with_trace_config(
            sdktrace::config().with_resource(opentelemetry_sdk::Resource::new(vec![
                opentelemetry::KeyValue::new("service.name", service_name.to_string()),
            ])),
        )
        .install_batch(runtime::Tokio)?;

    global::set_tracer_provider(tracer.provider().unwrap());

    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.level));

    let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);

    tracing_subscriber::registry()
        .with(filter)
        .with(telemetry)
        .with(fmt::layer())
        .init();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = TelemetryConfig::default();
        assert_eq!(config.level, "info");
        assert!(!config.json_output);
        assert!(config.otlp_endpoint.is_none());
    }
}
