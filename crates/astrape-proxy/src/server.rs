//! Actix Web HTTP server.
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
use actix_cors::Cors;
use actix_web::{http::StatusCode, web, App, HttpRequest, HttpResponse, HttpServer};
use anyhow::{Context, Result};
use futures::StreamExt;
use tracing::{debug, error, info};

#[derive(Clone)]
pub struct AppState {
    pub config: ProxyConfig,
    pub client: reqwest::Client,
}

pub async fn serve(config: ProxyConfig) -> Result<()> {
    let addr = format!("0.0.0.0:{}", config.port);
    info!(addr = %addr, "astrape-proxy listening");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(config.request_timeout_secs))
        .build()
        .context("failed to build reqwest client")?;

    let state = web::Data::new(AppState { config, client });

    HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .wrap(Cors::permissive())
            .route("/health", web::get().to(health_check))
            .route("/v1/messages", web::post().to(handle_messages))
            .route(
                "/v1/messages/count_tokens",
                web::post().to(handle_count_tokens),
            )
    })
    .bind(&addr)
    .with_context(|| format!("failed to bind {}", addr))?
    .run()
    .await
    .context("server error")?;

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
    state: web::Data<AppState>,
    req_http: HttpRequest,
    body: web::Json<MessagesRequest>,
) -> HttpResponse {
    let req = body.into_inner();
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
        Some(model) => handle_litellm_path(&state, req, model).await,
        None => handle_anthropic_passthrough(&state, &req_http, req).await,
    }
}

async fn handle_litellm_path(
    state: &AppState,
    mut req: MessagesRequest,
    target_model: String,
) -> HttpResponse {
    req.model = target_model;

    let provider = auth::model_to_provider(&req.model);
    let token = match auth::load_opencode_auth()
        .await
        .and_then(|s| auth::get_access_token(&s, provider))
    {
        Ok(t) => t,
        Err(e) => {
            error!(error = %e, provider, "OpenCode auth error");
            return HttpResponse::Unauthorized().body(e.to_string());
        }
    };

    let mut outgoing = match translation::convert_anthropic_to_litellm(&req) {
        Ok(v) => v,
        Err(e) => {
            error!(error = %e, "translation error");
            return HttpResponse::BadRequest().body(e.to_string());
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
            return HttpResponse::BadGateway().body(e.to_string());
        }
    };

    if !upstream.status().is_success() {
        let status = upstream.status();
        let text = upstream.text().await.unwrap_or_default();
        error!(%status, body = %text, "LiteLLM error");
        return HttpResponse::BadGateway().body(text);
    }

    if req.stream.unwrap_or(false) {
        let stream = streaming::handle_streaming(upstream, req).map(|r| {
            r.map(web::Bytes::from)
                .map_err(|e| actix_web::error::ErrorInternalServerError(e.to_string()))
        });

        return HttpResponse::Ok()
            .content_type("text/event-stream")
            .insert_header(("cache-control", "no-cache"))
            .streaming(stream);
    }

    let v: serde_json::Value = match upstream.json().await {
        Ok(v) => v,
        Err(e) => {
            error!(error = %e, "failed to decode LiteLLM response");
            return HttpResponse::BadGateway().body(e.to_string());
        }
    };

    match translation::convert_litellm_to_anthropic(v, &req) {
        Ok(out) => HttpResponse::Ok().json(out),
        Err(e) => {
            error!(error = %e, "response translation error");
            HttpResponse::BadGateway().body(e.to_string())
        }
    }
}

async fn handle_anthropic_passthrough(
    state: &AppState,
    req_http: &HttpRequest,
    req: MessagesRequest,
) -> HttpResponse {
    let headers = req_http.headers();
    let x_api_key = headers.get("x-api-key").and_then(|v| v.to_str().ok());
    let auth_header = headers.get("authorization").and_then(|v| v.to_str().ok());

    if x_api_key.is_none() && auth_header.is_none() {
        return HttpResponse::Unauthorized()
            .body("Missing authentication: provide x-api-key or Authorization header");
    }

    let url = "https://api.anthropic.com/v1/messages";

    let anthropic_version = headers
        .get("anthropic-version")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("2023-06-01");

    let mut request_builder = state
        .client
        .post(url)
        .header("anthropic-version", anthropic_version)
        .header("content-type", "application/json");

    if let Some(key_str) = x_api_key {
        request_builder = request_builder.header("x-api-key", key_str);
    }
    if let Some(auth_str) = auth_header {
        request_builder = request_builder.header("authorization", auth_str);
    }
    if let Some(beta) = headers.get("anthropic-beta").and_then(|v| v.to_str().ok()) {
        request_builder = request_builder.header("anthropic-beta", beta);
    }

    let upstream = match request_builder.json(&req).send().await {
        Ok(r) => r,
        Err(e) => {
            error!(error = %e, "Anthropic request failed");
            return HttpResponse::BadGateway().body(e.to_string());
        }
    };

    let status = upstream.status();
    let upstream_headers = upstream.headers().clone();
    let content_type = upstream_headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/json");

    if req.stream.unwrap_or(false) {
        let stream = upstream
            .bytes_stream()
            .map(|r| r.map_err(|e| actix_web::error::ErrorInternalServerError(e.to_string())));

        return HttpResponse::build(
            StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::OK),
        )
        .content_type(content_type)
        .insert_header(("cache-control", "no-cache"))
        .streaming(stream);
    }

    let body = match upstream.bytes().await {
        Ok(b) => b,
        Err(e) => {
            error!(error = %e, "failed to read Anthropic response");
            return HttpResponse::BadGateway().body(e.to_string());
        }
    };

    HttpResponse::build(StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::OK))
        .content_type(content_type)
        .body(body)
}

async fn handle_count_tokens(
    state: web::Data<AppState>,
    body: web::Json<TokenCountRequest>,
) -> HttpResponse {
    let req = body.into_inner();
    let model = req.model.clone();

    let provider = auth::model_to_provider(&req.model);
    let token = match auth::load_opencode_auth()
        .await
        .and_then(|s| auth::get_access_token(&s, provider))
    {
        Ok(t) => t,
        Err(e) => return HttpResponse::Unauthorized().body(e.to_string()),
    };

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
        Err(e) => return HttpResponse::BadRequest().body(e.to_string()),
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
        Err(e) => return HttpResponse::BadGateway().body(e.to_string()),
    };
    if !upstream.status().is_success() {
        let status = upstream.status();
        let text = upstream.text().await.unwrap_or_default();
        error!(%status, body = %text, "upstream error");
        return HttpResponse::BadGateway().body(text);
    }

    let v: serde_json::Value = match upstream.json().await {
        Ok(v) => v,
        Err(e) => return HttpResponse::BadGateway().body(e.to_string()),
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

    HttpResponse::Ok().json(TokenCountResponse {
        input_tokens: tokens,
    })
}
