use async_trait::async_trait;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use uira_protocol::{ApprovalRequirement, JsonSchema, SandboxPreference, ToolOutput};

use crate::{Tool, ToolContext, ToolError};

const DEFAULT_LIMIT: usize = 5;
const MAX_LIMIT: usize = 10;
const CACHE_TTL_SECS: u64 = 3600;
const RATE_LIMIT_WINDOW_SECS: u64 = 60;
const RATE_LIMIT_MAX_REQUESTS: usize = 10;
const FETCH_DEFAULT_MAX_CHARS: usize = 10000;
const FETCH_MAX_CHARS: usize = 50000;
const FETCH_MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

static STATE: Lazy<Mutex<WebState>> = Lazy::new(|| Mutex::new(WebState::new()));

#[derive(Default)]
struct WebState {
    cache: HashMap<String, CachedResults>,
    request_times: VecDeque<Instant>,
}

impl WebState {
    fn new() -> Self {
        Self {
            cache: HashMap::new(),
            request_times: VecDeque::new(),
        }
    }

    fn cleanup(&mut self) {
        let now = Instant::now();
        self.cache.retain(|_, v| v.expires_at > now);
        while let Some(ts) = self.request_times.front().copied() {
            if now.duration_since(ts).as_secs() >= RATE_LIMIT_WINDOW_SECS {
                self.request_times.pop_front();
            } else {
                break;
            }
        }
    }

    fn check_rate_limit(&self) -> Result<(), ToolError> {
        if self.request_times.len() >= RATE_LIMIT_MAX_REQUESTS {
            return Err(ToolError::ExecutionFailed {
                message: format!(
                    "Rate limit exceeded: max {RATE_LIMIT_MAX_REQUESTS} requests per {RATE_LIMIT_WINDOW_SECS} seconds"
                ),
            });
        }
        Ok(())
    }
}

struct CachedResults {
    results: Vec<SearchResult>,
    expires_at: Instant,
}

#[derive(Debug, Deserialize)]
struct WebSearchInput {
    query: String,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    mode: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
struct SearchResult {
    title: String,
    url: String,
    snippet: String,
}

#[derive(Debug, Serialize)]
struct WebSearchOutput {
    query: String,
    mode: String,
    cached: bool,
    provider: String,
    results: Vec<SearchResult>,
}

#[derive(Debug, Deserialize)]
struct FetchUrlInput {
    url: String,
    #[serde(default)]
    max_chars: Option<usize>,
}

#[derive(Debug, Serialize)]
struct FetchUrlOutput {
    url: String,
    content_type: String,
    title: Option<String>,
    content: String,
    truncated: bool,
}

pub struct WebSearchTool;

impl WebSearchTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WebSearchTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Search the web for up-to-date documentation snippets."
    }

    fn schema(&self) -> JsonSchema {
        JsonSchema::object()
            .property("query", JsonSchema::string().description("Search query"))
            .property(
                "limit",
                JsonSchema::number().description("Maximum number of results (1-10, default: 5)"),
            )
            .property(
                "mode",
                JsonSchema::string().description("Search mode: 'fast' (cached) or 'live'"),
            )
            .required(&["query"])
    }

    fn approval_requirement(&self, input: &serde_json::Value) -> ApprovalRequirement {
        let query = input
            .get("query")
            .and_then(|v| v.as_str())
            .unwrap_or("<unknown>");
        ApprovalRequirement::NeedsApproval {
            reason: format!("Search web for: {query}"),
        }
    }

    fn sandbox_preference(&self) -> SandboxPreference {
        SandboxPreference::Forbid
    }

    fn supports_parallel(&self) -> bool {
        false
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        let input: WebSearchInput =
            serde_json::from_value(input).map_err(|e| ToolError::InvalidInput {
                message: e.to_string(),
            })?;

        let query = input.query.trim();
        if query.is_empty() {
            return Err(ToolError::InvalidInput {
                message: "query must not be empty".to_string(),
            });
        }

        let limit = input.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
        let mode = input.mode.unwrap_or_else(|| "fast".to_string());
        if mode != "fast" && mode != "live" {
            return Err(ToolError::InvalidInput {
                message: "mode must be either 'fast' or 'live'".to_string(),
            });
        }

        let key = format!("{query}:{limit}");

        {
            let mut state = STATE.lock().await;
            state.cleanup();

            if mode == "fast" {
                if let Some(cached) = state.cache.get(&key) {
                    let out = WebSearchOutput {
                        query: query.to_string(),
                        mode,
                        cached: true,
                        provider: "duckduckgo".to_string(),
                        results: cached.results.clone(),
                    };
                    return Ok(ToolOutput::json(serde_json::to_value(out).unwrap()));
                }
            }

            state.check_rate_limit()?;
            state.request_times.push_back(Instant::now());
        }

        let results = duckduckgo_search(query, limit).await?;
        let out = WebSearchOutput {
            query: query.to_string(),
            mode: mode.clone(),
            cached: false,
            provider: "duckduckgo".to_string(),
            results: results.clone(),
        };

        if mode == "fast" {
            let mut state = STATE.lock().await;
            state.cache.insert(
                key,
                CachedResults {
                    results,
                    expires_at: Instant::now() + Duration::from_secs(CACHE_TTL_SECS),
                },
            );
        }

        Ok(ToolOutput::json(serde_json::to_value(out).unwrap()))
    }
}

pub struct FetchUrlTool;

impl FetchUrlTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FetchUrlTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for FetchUrlTool {
    fn name(&self) -> &str {
        "fetch_url"
    }

    fn description(&self) -> &str {
        "Fetch a URL and return cleaned text content."
    }

    fn schema(&self) -> JsonSchema {
        JsonSchema::object()
            .property("url", JsonSchema::string().description("URL to fetch"))
            .property(
                "max_chars",
                JsonSchema::number()
                    .description("Max returned characters (default 10000, max 50000)"),
            )
            .required(&["url"])
    }

    fn approval_requirement(&self, input: &serde_json::Value) -> ApprovalRequirement {
        let url = input
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("<unknown>");
        ApprovalRequirement::NeedsApproval {
            reason: format!("Fetch URL: {url}"),
        }
    }

    fn sandbox_preference(&self) -> SandboxPreference {
        SandboxPreference::Forbid
    }

    fn supports_parallel(&self) -> bool {
        false
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        let input: FetchUrlInput =
            serde_json::from_value(input).map_err(|e| ToolError::InvalidInput {
                message: e.to_string(),
            })?;
        let max_chars = input
            .max_chars
            .unwrap_or(FETCH_DEFAULT_MAX_CHARS)
            .clamp(1, FETCH_MAX_CHARS);

        let url = reqwest::Url::parse(&input.url).map_err(|e| ToolError::InvalidInput {
            message: format!("Invalid URL: {e}"),
        })?;
        validate_fetch_url(&url)?;

        {
            let mut state = STATE.lock().await;
            state.cleanup();
            state.check_rate_limit()?;
            state.request_times.push_back(Instant::now());
        }

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent("uira/0.1 (+https://github.com/junhoyeo/uira)")
            .build()
            .map_err(|e| ToolError::ExecutionFailed {
                message: format!("Failed to initialize HTTP client: {e}"),
            })?;

        let response = client
            .get(url)
            .send()
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                message: format!("Request failed: {e}"),
            })?;

        if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(ToolError::ExecutionFailed {
                message: "URL fetch was rate-limited by remote server (HTTP 429)".to_string(),
            });
        }
        if !response.status().is_success() {
            return Err(ToolError::ExecutionFailed {
                message: format!("URL fetch failed with HTTP {}", response.status()),
            });
        }

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();

        let body_bytes = response
            .bytes()
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                message: format!("Failed to read response body: {e}"),
            })?;

        if body_bytes.len() > FETCH_MAX_RESPONSE_BYTES {
            return Err(ToolError::ExecutionFailed {
                message: format!(
                    "Response too large: {} bytes exceeds limit of {} bytes",
                    body_bytes.len(),
                    FETCH_MAX_RESPONSE_BYTES
                ),
            });
        }

        let body = String::from_utf8_lossy(&body_bytes).to_string();

        let title = extract_title(&body);
        let text = normalize_whitespace(&decode_html_entities(&strip_html_tags(&body)));
        let truncated = text.chars().count() > max_chars;
        let content = if truncated {
            text.chars().take(max_chars).collect()
        } else {
            text
        };

        let out = FetchUrlOutput {
            url: input.url,
            content_type,
            title,
            content,
            truncated,
        };
        Ok(ToolOutput::json(serde_json::to_value(out).unwrap()))
    }
}

fn extract_title(html: &str) -> Option<String> {
    let title_re = Regex::new(r"(?is)<title[^>]*>(.*?)</title>").expect("valid regex");
    let title = title_re
        .captures(html)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str())
        .unwrap_or_default();
    let title = normalize_whitespace(&decode_html_entities(&strip_html_tags(title)));
    if title.is_empty() {
        None
    } else {
        Some(title)
    }
}

async fn duckduckgo_search(query: &str, limit: usize) -> Result<Vec<SearchResult>, ToolError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent("uira/0.1 (+https://github.com/junhoyeo/uira)")
        .build()
        .map_err(|e| ToolError::ExecutionFailed {
            message: format!("Failed to initialize HTTP client: {e}"),
        })?;

    let mut url = reqwest::Url::parse("https://duckduckgo.com/html/").map_err(|e| {
        ToolError::ExecutionFailed {
            message: format!("Failed to build search URL: {e}"),
        }
    })?;
    url.query_pairs_mut().append_pair("q", query);

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| ToolError::ExecutionFailed {
            message: format!("Search request failed: {e}"),
        })?;

    if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(ToolError::ExecutionFailed {
            message: "Search provider rate-limited this request (HTTP 429)".to_string(),
        });
    }
    if !response.status().is_success() {
        return Err(ToolError::ExecutionFailed {
            message: format!("Search provider returned HTTP {}", response.status()),
        });
    }

    let html = response
        .text()
        .await
        .map_err(|e| ToolError::ExecutionFailed {
            message: format!("Failed to read search response body: {e}"),
        })?;

    let re = Regex::new(
        r#"(?s)<a[^>]*class=\"[^\"]*result__a[^\"]*\"[^>]*href=\"(?P<url>[^\"]+)\"[^>]*>(?P<title>.*?)</a>.*?<a[^>]*class=\"[^\"]*result__snippet[^\"]*\"[^>]*>(?P<snippet>.*?)</a>"#,
    )
    .expect("valid regex");

    let mut out = Vec::new();
    for cap in re.captures_iter(&html) {
        if out.len() >= limit {
            break;
        }
        let url = cap.name("url").map(|m| m.as_str()).unwrap_or_default();
        let title = cap.name("title").map(|m| m.as_str()).unwrap_or_default();
        let snippet = cap.name("snippet").map(|m| m.as_str()).unwrap_or_default();

        let url = decode_ddg_url(url);
        if url.is_empty() {
            continue;
        }

        out.push(SearchResult {
            title: normalize_whitespace(&decode_html_entities(&strip_html_tags(title))),
            url,
            snippet: normalize_whitespace(&decode_html_entities(&strip_html_tags(snippet))),
        });
    }

    Ok(out)
}

fn decode_ddg_url(url: &str) -> String {
    if let Ok(parsed) = reqwest::Url::parse(url) {
        if parsed.path() == "/l/" {
            for (k, v) in parsed.query_pairs() {
                if k == "uddg" {
                    return v.to_string();
                }
            }
        }
    }
    url.to_string()
}

fn strip_html_tags(input: &str) -> String {
    Regex::new(r"(?is)<[^>]+>")
        .expect("valid regex")
        .replace_all(input, " ")
        .to_string()
}

fn decode_html_entities(input: &str) -> String {
    input
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

fn normalize_whitespace(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn validate_fetch_url(url: &reqwest::Url) -> Result<(), ToolError> {
    match url.scheme() {
        "http" | "https" => {}
        other => {
            return Err(ToolError::InvalidInput {
                message: format!("Unsupported URL scheme: {other}"),
            });
        }
    }

    let host = url
        .host_str()
        .ok_or_else(|| ToolError::InvalidInput {
            message: "URL must include a hostname".to_string(),
        })?
        .to_ascii_lowercase();

    if host == "localhost" || host.ends_with(".local") {
        return Err(ToolError::ExecutionFailed {
            message: "Blocked local hostname for fetch_url".to_string(),
        });
    }

    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_private_or_local_ip(ip) {
            return Err(ToolError::ExecutionFailed {
                message: "Blocked private or local IP address for fetch_url".to_string(),
            });
        }
    }

    Ok(())
}

fn is_private_or_local_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_multicast()
                || v4 == Ipv4Addr::new(169, 254, 169, 254)
                || v4 == Ipv4Addr::new(0, 0, 0, 0)
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || v6.is_unique_local()
                || v6.is_unicast_link_local()
                || v6 == Ipv6Addr::LOCALHOST
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_localhost_urls() {
        let url = reqwest::Url::parse("http://localhost:8080").unwrap();
        assert!(validate_fetch_url(&url).is_err());
    }

    #[test]
    fn blocks_private_ip_urls() {
        let url = reqwest::Url::parse("http://127.0.0.1/api").unwrap();
        assert!(validate_fetch_url(&url).is_err());
    }

    #[test]
    fn allows_public_https_urls() {
        let url = reqwest::Url::parse("https://example.com/docs").unwrap();
        assert!(validate_fetch_url(&url).is_ok());
    }
}
