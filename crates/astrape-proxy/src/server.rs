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
    http::{header, HeaderMap, HeaderValue, Response, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use futures::StreamExt;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::{debug, error, info};

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

fn extract_agent_name(metadata: &Option<serde_json::Value>) -> Option<String> {
    metadata
        .as_ref()
        .and_then(|m| m.get("agent"))
        .and_then(|a| a.as_str())
        .map(|s| s.to_string())
}

async fn handle_messages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<MessagesRequest>,
) -> impl IntoResponse {
    let agent_name = extract_agent_name(&req.metadata);
    let agent_model = agent_name
        .as_ref()
        .and_then(|name| state.config.get_model_for_agent(name))
        .map(|s| s.to_string());

    debug!(
        ?agent_name,
        ?agent_model,
        original_model = %req.model,
        "routing decision"
    );

    match agent_model {
        Some(model) => handle_litellm_path(state, req, model).await,
        None => handle_anthropic_passthrough(state, headers, req).await,
    }
}

async fn handle_litellm_path(
    state: AppState,
    mut req: MessagesRequest,
    target_model: String,
) -> axum::response::Response {
    req.model = target_model;

    let provider = auth::model_to_provider(&req.model);
    let token = match auth::load_opencode_auth()
        .await
        .and_then(|s| auth::get_access_token(&s, provider))
    {
        Ok(t) => t,
        Err(e) => {
            error!(error = %e, provider, "OpenCode auth error");
            return (StatusCode::UNAUTHORIZED, e.to_string()).into_response();
        }
    };

    let mut outgoing = match translation::convert_anthropic_to_litellm(&req) {
        Ok(v) => v,
        Err(e) => {
            error!(error = %e, "translation error");
            return (StatusCode::BAD_REQUEST, e.to_string()).into_response();
        }
    };

    if let Some(obj) = outgoing.as_object_mut() {
        obj.insert("api_key".to_string(), serde_json::Value::String(token));
    }

    let url = format!(
        "{}/v1/chat/completions",
        state.config.litellm_base_url_trimmed()
    );

    let upstream = match state.client.post(&url).json(&outgoing).send().await {
        Ok(r) => r,
        Err(e) => {
            error!(error = %e, "LiteLLM request failed");
            return (StatusCode::BAD_GATEWAY, e.to_string()).into_response();
        }
    };

    if !upstream.status().is_success() {
        let status = upstream.status();
        let text = upstream.text().await.unwrap_or_default();
        error!(%status, body = %text, "LiteLLM error");
        return (StatusCode::BAD_GATEWAY, text).into_response();
    }

    if req.stream.unwrap_or(false) {
        let stream = streaming::handle_streaming(upstream, req).map(|r| r.map(Bytes::from));
        let mut resp = Response::new(Body::from_stream(stream));
        resp.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/event-stream"),
        );
        resp.headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
        return resp.into_response();
    }

    let v: serde_json::Value = match upstream.json().await {
        Ok(v) => v,
        Err(e) => {
            error!(error = %e, "failed to decode LiteLLM response");
            return (StatusCode::BAD_GATEWAY, e.to_string()).into_response();
        }
    };

    match translation::convert_litellm_to_anthropic(v, &req) {
        Ok(out) => Json(out).into_response(),
        Err(e) => {
            error!(error = %e, "response translation error");
            (StatusCode::BAD_GATEWAY, e.to_string()).into_response()
        }
    }
}

async fn handle_anthropic_passthrough(
    state: AppState,
    headers: HeaderMap,
    req: MessagesRequest,
) -> axum::response::Response {
    let auth_header = match headers.get(header::AUTHORIZATION) {
        Some(h) => h.clone(),
        None => {
            return (StatusCode::UNAUTHORIZED, "Missing Authorization header").into_response();
        }
    };

    let url = "https://api.anthropic.com/v1/messages";

    let mut request_builder = state
        .client
        .post(url)
        .header(header::AUTHORIZATION, auth_header)
        .header("anthropic-version", "2023-06-01")
        .header(header::CONTENT_TYPE, "application/json");

    if let Some(api_key) = headers.get("x-api-key") {
        request_builder = request_builder.header("x-api-key", api_key.clone());
    }

    let upstream = match request_builder.json(&req).send().await {
        Ok(r) => r,
        Err(e) => {
            error!(error = %e, "Anthropic request failed");
            return (StatusCode::BAD_GATEWAY, e.to_string()).into_response();
        }
    };

    let status = upstream.status();
    let upstream_headers = upstream.headers().clone();

    if req.stream.unwrap_or(false) {
        let stream = upstream
            .bytes_stream()
            .map(|r| r.map_err(std::io::Error::other));
        let mut resp = Response::new(Body::from_stream(stream));
        *resp.status_mut() = status;
        if let Some(ct) = upstream_headers.get(header::CONTENT_TYPE) {
            resp.headers_mut().insert(header::CONTENT_TYPE, ct.clone());
        }
        resp.headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
        return resp.into_response();
    }

    let body = match upstream.bytes().await {
        Ok(b) => b,
        Err(e) => {
            error!(error = %e, "failed to read Anthropic response");
            return (StatusCode::BAD_GATEWAY, e.to_string()).into_response();
        }
    };

    let mut resp = Response::new(Body::from(body));
    *resp.status_mut() = status;
    if let Some(ct) = upstream_headers.get(header::CONTENT_TYPE) {
        resp.headers_mut().insert(header::CONTENT_TYPE, ct.clone());
    }
    resp.into_response()
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
        .unwrap_or_else(|| {
            tracing::warn!(
                response = ?v,
                "token count fields missing from upstream response, defaulting to 0"
            );
            0
        }) as u32;

    Json(TokenCountResponse {
        input_tokens: tokens,
    })
    .into_response()
}
