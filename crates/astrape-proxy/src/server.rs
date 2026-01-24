//! Axum HTTP server.
//!
//! Exposes Anthropic-compatible endpoints:
//! - `POST /v1/messages`
//! - `POST /v1/messages/count_tokens`
//! - `GET /health`

use crate::{
    auth,
    config::ProxyConfig,
    streaming, translation,
    types::{MessagesRequest, TokenCountRequest, TokenCountResponse},
};
use anyhow::{Context, Result};
use axum::{
    body::{Body, Bytes},
    extract::State,
    http::{HeaderValue, Response, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use futures::StreamExt;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::{error, info};

#[derive(Clone)]
struct AppState {
    config: ProxyConfig,
    client: reqwest::Client,
}

/// Create an Axum router for the proxy.
pub fn create_app(config: ProxyConfig) -> Router {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(config.request_timeout_secs))
        .build()
        .expect("failed to build reqwest client");

    let state = AppState { config, client };

    Router::new()
        .route("/health", get(health_check))
        .route("/v1/messages", post(handle_messages))
        .route("/v1/messages/count_tokens", post(handle_count_tokens))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Bind and serve the proxy.
pub async fn serve(config: ProxyConfig) -> Result<()> {
    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], config.port));
    let app = create_app(config);
    info!(%addr, "astrape-proxy listening");
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind {}", addr))?;
    axum::serve(listener, app).await.context("server error")?;
    Ok(())
}

async fn health_check() -> &'static str {
    "OK"
}

async fn handle_messages(
    State(state): State<AppState>,
    Json(mut req): Json<MessagesRequest>,
) -> impl IntoResponse {
    // 1) Map Claude model aliases to configured targets.
    req.model = state.config.map_model(&req.model);

    // 2) Load provider token from OpenCode auth store.
    let provider = auth::model_to_provider(&req.model);
    let token = match auth::load_opencode_auth()
        .await
        .and_then(|s| auth::get_access_token(&s, provider))
    {
        Ok(t) => t,
        Err(e) => {
            error!(error = %e, provider, "auth error");
            return (StatusCode::UNAUTHORIZED, e.to_string()).into_response();
        }
    };

    // 3) Translate request to LiteLLM/OpenAI format.
    let mut outgoing = match translation::convert_anthropic_to_litellm(&req) {
        Ok(v) => v,
        Err(e) => {
            error!(error = %e, "translation error");
            return (StatusCode::BAD_REQUEST, e.to_string()).into_response();
        }
    };

    // Pass provider token in-body. This avoids conflicting with LiteLLM proxy's
    // own `Authorization` handling.
    if let Some(obj) = outgoing.as_object_mut() {
        obj.insert("api_key".to_string(), serde_json::Value::String(token));
    }

    let url = format!(
        "{}/v1/chat/completions",
        state.config.litellm_base_url_trimmed()
    );

    // 4) Proxy.
    let upstream = match state.client.post(url).json(&outgoing).send().await {
        Ok(r) => r,
        Err(e) => {
            error!(error = %e, "upstream request failed");
            return (StatusCode::BAD_GATEWAY, e.to_string()).into_response();
        }
    };

    if !upstream.status().is_success() {
        let status = upstream.status();
        let text = upstream.text().await.unwrap_or_else(|_| "".to_string());
        error!(%status, body = %text, "upstream error");
        return (StatusCode::BAD_GATEWAY, text).into_response();
    }

    if req.stream.unwrap_or(false) {
        // Streaming: convert upstream SSE -> Anthropic SSE.
        let stream = streaming::handle_streaming(upstream, req).map(|r| r.map(Bytes::from));

        let mut resp = Response::new(Body::from_stream(stream));
        resp.headers_mut().insert(
            axum::http::header::CONTENT_TYPE,
            HeaderValue::from_static("text/event-stream"),
        );
        return resp.into_response();
    }

    // Non-streaming: JSON response conversion.
    let v: serde_json::Value = match upstream.json().await {
        Ok(v) => v,
        Err(e) => {
            error!(error = %e, "failed to decode upstream json");
            return (StatusCode::BAD_GATEWAY, e.to_string()).into_response();
        }
    };
    let out = match translation::convert_litellm_to_anthropic(v, &req) {
        Ok(r) => r,
        Err(e) => {
            error!(error = %e, "response translation error");
            return (StatusCode::BAD_GATEWAY, e.to_string()).into_response();
        }
    };

    Json(out).into_response()
}

async fn handle_count_tokens(
    State(state): State<AppState>,
    Json(req): Json<TokenCountRequest>,
) -> impl IntoResponse {
    // For token counting, use the model as-is (no agent-based mapping needed)
    let model = req.model.clone();

    let provider = auth::model_to_provider(&req.model);
    let token = match auth::load_opencode_auth()
        .await
        .and_then(|s| auth::get_access_token(&s, provider))
    {
        Ok(t) => t,
        Err(e) => return (StatusCode::UNAUTHORIZED, e.to_string()).into_response(),
    };

    // Reuse the messages translation by building a MessagesRequest-like shape.
    let msgs_req = MessagesRequest {
        model,
        messages: req.messages,
        system: req.system,
        max_tokens: 1,
        stream: None,
        temperature: None,
        top_p: None,
        top_k: None,
        stop_sequences: None,
        tools: req.tools,
        tool_choice: None,
        thinking: None,
        metadata: None,
    };
    let mut outgoing = match translation::convert_anthropic_to_litellm(&msgs_req) {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    };
    if let Some(obj) = outgoing.as_object_mut() {
        obj.insert("api_key".to_string(), serde_json::Value::String(token));
    }

    let url = format!(
        "{}/utils/token_counter",
        state.config.litellm_base_url_trimmed()
    );
    let upstream = match state.client.post(url).json(&outgoing).send().await {
        Ok(r) => r,
        Err(e) => return (StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    };
    if !upstream.status().is_success() {
        let status = upstream.status();
        let text = upstream.text().await.unwrap_or_else(|_| "".to_string());
        error!(%status, body = %text, "upstream error");
        return (StatusCode::BAD_GATEWAY, text).into_response();
    }

    let v: serde_json::Value = match upstream.json().await {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    };

    let tokens = v
        .get("token_count")
        .or_else(|| v.get("tokens"))
        .or_else(|| v.get("count"))
        .and_then(|x| x.as_u64())
        .unwrap_or(0) as u32;

    Json(TokenCountResponse {
        input_tokens: tokens,
    })
    .into_response()
}
