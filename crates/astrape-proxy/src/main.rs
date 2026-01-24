//! `astrape-proxy` binary entrypoint.
//!
//! This starts the Axum server using configuration from environment variables.

use astrape_proxy::{serve, ProxyConfig};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Respect `RUST_LOG` if set; otherwise default to proxy-friendly info.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = ProxyConfig::default();
    serve(config).await
}
