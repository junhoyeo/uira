use async_trait::async_trait;
use futures::StreamExt;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::Path;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use uira_protocol::{ApprovalRequirement, JsonSchema, SandboxPreference, ToolOutput};

use crate::{Tool, ToolContext, ToolError};

const DEFAULT_LIMIT: usize = 5;
const MAX_LIMIT: usize = 10;
const DEFAULT_NUM_RESULTS: usize = 8;
const CACHE_TTL_SECS: u64 = 3600;
const RATE_LIMIT_WINDOW_SECS: u64 = 60;
const RATE_LIMIT_MAX_REQUESTS: usize = 10;
const FETCH_DEFAULT_MAX_CHARS: usize = 10000;
const FETCH_MAX_CHARS: usize = 50000;
const FETCH_MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const DEFAULT_PROVIDER: &str = "exa";
const EXA_MCP_URL: &str = "https://mcp.exa.ai/mcp";
const EXA_SEARCH_TIMEOUT_SECS: u64 = 25;
const EXA_CODE_SEARCH_TIMEOUT_SECS: u64 = 30;
const GREP_APP_MCP_URL: &str = "https://mcp.grep.app";
const GREP_APP_TIMEOUT_SECS: u64 = 25;

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

    fn cleanup(&mut self, rate_limit_window_secs: u64) {
        let now = Instant::now();
        self.cache.retain(|_, v| v.expires_at > now);
        while let Some(ts) = self.request_times.front().copied() {
            if now.duration_since(ts).as_secs() >= rate_limit_window_secs {
                self.request_times.pop_front();
            } else {
                break;
            }
        }
    }

    fn check_rate_limit(&self, max_requests: usize, window_secs: u64) -> Result<(), ToolError> {
        if self.request_times.len() >= max_requests {
            return Err(ToolError::ExecutionFailed {
                message: format!(
                    "Rate limit exceeded: max {max_requests} requests per {window_secs} seconds"
                ),
            });
        }
        Ok(())
    }
}

struct CachedResults {
    results: Vec<SearchResult>,
    output: Option<String>,
    expires_at: Instant,
}

#[derive(Debug, Clone)]
struct RuntimeConfig {
    enabled: bool,
    provider: String,
    cache_ttl_secs: u64,
    rate_limit_max_requests: usize,
    rate_limit_window_secs: u64,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            provider: DEFAULT_PROVIDER.to_string(),
            cache_ttl_secs: CACHE_TTL_SECS,
            rate_limit_max_requests: RATE_LIMIT_MAX_REQUESTS,
            rate_limit_window_secs: RATE_LIMIT_WINDOW_SECS,
        }
    }
}

#[derive(Debug, Deserialize)]
struct UiraConfigFile {
    #[serde(default)]
    tools: Option<ToolsConfig>,
}

#[derive(Debug, Deserialize)]
struct ToolsConfig {
    #[serde(default)]
    web_search: Option<WebSearchConfig>,
}

#[derive(Debug, Deserialize)]
struct WebSearchConfig {
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    cache_ttl: Option<u64>,
    #[serde(default)]
    rate_limit_max_requests: Option<usize>,
    #[serde(default)]
    rate_limit_window_secs: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct WebSearchInput {
    query: String,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    num_results: Option<usize>,
    #[serde(default)]
    search_type: Option<String>,
    #[serde(default)]
    livecrawl: Option<String>,
    #[serde(default)]
    context_max_chars: Option<usize>,
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
    output: Option<String>,
    results: Vec<SearchResult>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct CodeSearchInput {
    query: String,
    #[serde(default)]
    tokens_num: Option<usize>,
}

#[derive(Serialize)]
struct ExaMcpRequest {
    jsonrpc: String,
    id: u32,
    method: String,
    params: ExaMcpParams,
}

#[derive(Serialize)]
struct ExaMcpParams {
    name: String,
    arguments: serde_json::Value,
}

#[derive(Deserialize)]
struct ExaMcpResponse {
    result: Option<ExaMcpResult>,
}

#[derive(Deserialize)]
struct ExaMcpResult {
    content: Vec<ExaMcpContent>,
}

#[derive(Deserialize)]
struct ExaMcpContent {
    #[serde(rename = "type")]
    content_type: String,
    text: String,
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
pub struct CodeSearchTool;
pub struct GrepAppTool;

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GrepAppInput {
    query: String,
    #[serde(default)]
    language: Option<Vec<String>>,
    #[serde(default)]
    repo: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default, rename = "matchCase")]
    match_case: Option<bool>,
    #[serde(default, rename = "matchWholeWords")]
    match_whole_words: Option<bool>,
    #[serde(default, rename = "useRegexp")]
    use_regexp: Option<bool>,
}

#[derive(Serialize)]
struct GrepAppMcpRequest {
    jsonrpc: String,
    id: u32,
    method: String,
    params: GrepAppMcpParams,
}

#[derive(Serialize)]
struct GrepAppMcpParams {
    name: String,
    arguments: serde_json::Value,
}

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

impl CodeSearchTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CodeSearchTool {
    fn default() -> Self {
        Self::new()
    }
}

impl GrepAppTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for GrepAppTool {
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
        "Search the web using Exa hosted MCP for up-to-date documentation snippets, with DuckDuckGo fallback support."
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
            .property(
                "num_results",
                JsonSchema::number().description("Number of search results to return (default: 8)"),
            )
            .property(
                "search_type",
                JsonSchema::string()
                    .description("Search type: 'auto' (default), 'fast', or 'deep'"),
            )
            .property(
                "livecrawl",
                JsonSchema::string()
                    .description("Live crawl mode: 'fallback' (default) or 'preferred'"),
            )
            .property(
                "context_max_chars",
                JsonSchema::number().description(
                    "Maximum characters for context optimized for LLMs (default: provider default)",
                ),
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
        ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        let runtime = load_runtime_config(&ctx.cwd);
        if !runtime.enabled {
            return Err(ToolError::ExecutionFailed {
                message: "web_search is disabled by uira.yml tools.web_search.enabled=false"
                    .to_string(),
            });
        }
        if runtime.provider != "exa" && runtime.provider != "duckduckgo" {
            return Err(ToolError::ExecutionFailed {
                message: format!(
                    "Unsupported web_search provider '{}'; supported providers are 'exa' and 'duckduckgo'",
                    runtime.provider
                ),
            });
        }

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

        let num_results = input
            .num_results
            .unwrap_or(DEFAULT_NUM_RESULTS)
            .clamp(1, MAX_LIMIT);
        let search_type = input.search_type.unwrap_or_else(|| "auto".to_string());
        if search_type != "auto" && search_type != "fast" && search_type != "deep" {
            return Err(ToolError::InvalidInput {
                message: "search_type must be one of: 'auto', 'fast', 'deep'".to_string(),
            });
        }
        let livecrawl = input.livecrawl.unwrap_or_else(|| "fallback".to_string());
        if livecrawl != "fallback" && livecrawl != "preferred" {
            return Err(ToolError::InvalidInput {
                message: "livecrawl must be either 'fallback' or 'preferred'".to_string(),
            });
        }

        let key = if runtime.provider == "exa" {
            format!(
                "exa:{query}:{num_results}:{search_type}:{livecrawl}:{}",
                input.context_max_chars.unwrap_or(0)
            )
        } else {
            format!("duckduckgo:{query}:{limit}")
        };

        {
            let mut state = STATE.lock().await;
            state.cleanup(runtime.rate_limit_window_secs);

            if mode == "fast" {
                if let Some(cached) = state.cache.get(&key) {
                    let out = WebSearchOutput {
                        query: query.to_string(),
                        mode,
                        cached: true,
                        provider: runtime.provider.clone(),
                        output: cached.output.clone(),
                        results: cached.results.clone(),
                    };
                    return serde_json::to_value(out)
                        .map(ToolOutput::json)
                        .map_err(|e| ToolError::ExecutionFailed {
                            message: format!("Failed to serialize output: {}", e),
                        });
                }
            }

            state.check_rate_limit(
                runtime.rate_limit_max_requests,
                runtime.rate_limit_window_secs,
            )?;
            state.request_times.push_back(Instant::now());
        }

        let provider = runtime.provider.clone();
        let (results, output, effective_provider) = if provider == "exa" {
            match exa_search(
                query,
                num_results,
                &search_type,
                &livecrawl,
                input.context_max_chars,
            )
            .await
            {
                Ok(text) => {
                    if text.trim().is_empty() {
                        (Vec::new(), Some("No search results found".to_string()), "exa")
                    } else {
                        (Vec::new(), Some(text), "exa")
                    }
                }
                Err(err) => {
                    tracing::warn!(
                        "Exa web search failed for query '{}': {}. Falling back to DuckDuckGo.",
                        query,
                        err
                    );
                    (duckduckgo_search(query, limit).await?, None, "duckduckgo")
                }
            }
        } else {
            (duckduckgo_search(query, limit).await?, None, "duckduckgo")
        };

        let out = WebSearchOutput {
            query: query.to_string(),
            mode: mode.clone(),
            cached: false,
            provider: effective_provider.to_string(),
            output,
            results: results.clone(),
        };

        if mode == "fast" {
            let mut state = STATE.lock().await;
            state.cache.insert(
                key,
                CachedResults {
                    results,
                    output: out.output.clone(),
                    expires_at: Instant::now() + Duration::from_secs(runtime.cache_ttl_secs),
                },
            );
        }

        serde_json::to_value(out)
            .map(ToolOutput::json)
            .map_err(|e| ToolError::ExecutionFailed {
                message: format!("Failed to serialize output: {}", e),
            })
    }
}

#[async_trait]
impl Tool for CodeSearchTool {
    fn name(&self) -> &str {
        "code_search"
    }

    fn description(&self) -> &str {
        "Search and get relevant code context for programming tasks using Exa Code API. Provides high-quality code examples, documentation, and API references for libraries, SDKs, and frameworks."
    }

    fn schema(&self) -> JsonSchema {
        JsonSchema::object()
            .property("query", JsonSchema::string().description("Code search query"))
            .property(
                "tokens_num",
                JsonSchema::number().description(
                    "Number of tokens to return (1000-50000, default: 5000)",
                ),
            )
            .required(&["query"])
    }

    fn approval_requirement(&self, input: &serde_json::Value) -> ApprovalRequirement {
        let query = input
            .get("query")
            .and_then(|v| v.as_str())
            .unwrap_or("<unknown>");
        ApprovalRequirement::NeedsApproval {
            reason: format!("Search code context for: {query}"),
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
        ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        let runtime = load_runtime_config(&ctx.cwd);
        if !runtime.enabled {
            return Err(ToolError::ExecutionFailed {
                message: "code_search is disabled by uira.yml tools.web_search.enabled=false"
                    .to_string(),
            });
        }

        let input: CodeSearchInput =
            serde_json::from_value(input).map_err(|e| ToolError::InvalidInput {
                message: e.to_string(),
            })?;

        let query = input.query.trim();
        if query.is_empty() {
            return Err(ToolError::InvalidInput {
                message: "query must not be empty".to_string(),
            });
        }

        let tokens_num = input.tokens_num.unwrap_or(5000);
        if !(1000..=50000).contains(&tokens_num) {
            return Err(ToolError::InvalidInput {
                message: "tokens_num must be between 1000 and 50000".to_string(),
            });
        }

        {
            let mut state = STATE.lock().await;
            state.cleanup(runtime.rate_limit_window_secs);
            state.check_rate_limit(
                runtime.rate_limit_max_requests,
                runtime.rate_limit_window_secs,
            )?;
            state.request_times.push_back(Instant::now());
        }

        let output = exa_code_search(query, tokens_num).await?;
        Ok(ToolOutput::json(serde_json::json!({
            "query": query,
            "tokens_num": tokens_num,
            "provider": "exa",
            "output": output
        })))
    }
}

#[async_trait]
impl Tool for GrepAppTool {
    fn name(&self) -> &str {
        "grep_app"
    }

    fn description(&self) -> &str {
        "Find real-world code examples from over a million public GitHub repositories. Searches for literal code patterns (like grep), not keywords. Use actual code that would appear in files. Filter by language, repository, or file path."
    }

    fn schema(&self) -> JsonSchema {
        JsonSchema::object()
            .property(
                "query",
                JsonSchema::string().description(
                    "The literal code pattern to search for (e.g., 'useState(', 'export function'). Use actual code that would appear in files, not keywords or questions.",
                ),
            )
            .property(
                "language",
                JsonSchema::array(JsonSchema::string()).description(
                    "Filter by programming language. Examples: ['TypeScript', 'TSX'], ['Python'], ['Rust']",
                ),
            )
            .property(
                "repo",
                JsonSchema::string().description(
                    "Filter by repository. Examples: 'facebook/react', 'vercel/ai'. Can match partial names like 'vercel/'",
                ),
            )
            .property(
                "path",
                JsonSchema::string().description(
                    "Filter by file path. Examples: 'src/components/Button.tsx', '/route.ts'",
                ),
            )
            .property(
                "matchCase",
                JsonSchema::boolean().description("Whether the search should be case sensitive (default: false)"),
            )
            .property(
                "matchWholeWords",
                JsonSchema::boolean().description("Whether to match whole words only (default: false)"),
            )
            .property(
                "useRegexp",
                JsonSchema::boolean().description(
                    "Whether to interpret the query as a regular expression (default: false). Prefix with '(?s)' to match across multiple lines.",
                ),
            )
            .required(&["query"])
    }

    fn approval_requirement(&self, input: &serde_json::Value) -> ApprovalRequirement {
        let query = input
            .get("query")
            .and_then(|v| v.as_str())
            .unwrap_or("<unknown>");
        ApprovalRequirement::NeedsApproval {
            reason: format!("Search GitHub code for: {query}"),
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
        ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        let runtime = load_runtime_config(&ctx.cwd);
        if !runtime.enabled {
            return Err(ToolError::ExecutionFailed {
                message: "grep_app is disabled by uira.yml tools.web_search.enabled=false"
                    .to_string(),
            });
        }

        let input: GrepAppInput =
            serde_json::from_value(input).map_err(|e| ToolError::InvalidInput {
                message: e.to_string(),
            })?;

        let query = input.query.trim();
        if query.is_empty() {
            return Err(ToolError::InvalidInput {
                message: "query must not be empty".to_string(),
            });
        }

        {
            let mut state = STATE.lock().await;
            state.cleanup(runtime.rate_limit_window_secs);
            state.check_rate_limit(
                runtime.rate_limit_max_requests,
                runtime.rate_limit_window_secs,
            )?;
            state.request_times.push_back(Instant::now());
        }

        let output = grep_app_search(
            query,
            input.language.as_deref(),
            input.repo.as_deref(),
            input.path.as_deref(),
            input.match_case.unwrap_or(false),
            input.match_whole_words.unwrap_or(false),
            input.use_regexp.unwrap_or(false),
        )
        .await?;

        Ok(ToolOutput::json(serde_json::json!({
            "query": query,
            "provider": "grep.app",
            "output": output
        })))
    }
}

async fn grep_app_search(
    query: &str,
    language: Option<&[String]>,
    repo: Option<&str>,
    path: Option<&str>,
    match_case: bool,
    match_whole_words: bool,
    use_regexp: bool,
) -> Result<String, ToolError> {
    let mut args = serde_json::json!({
        "query": query,
        "matchCase": match_case,
        "matchWholeWords": match_whole_words,
        "useRegexp": use_regexp,
    });

    if let Some(lang) = language {
        args["language"] = serde_json::json!(lang);
    }
    if let Some(r) = repo {
        args["repo"] = serde_json::json!(r);
    }
    if let Some(p) = path {
        args["path"] = serde_json::json!(p);
    }

    let request = GrepAppMcpRequest {
        jsonrpc: "2.0".to_string(),
        id: 1,
        method: "tools/call".to_string(),
        params: GrepAppMcpParams {
            name: "searchGitHub".to_string(),
            arguments: args,
        },
    };

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(GREP_APP_TIMEOUT_SECS))
        .user_agent("uira/0.1 (+https://github.com/junhoyeo/uira)")
        .build()
        .map_err(|e| ToolError::ExecutionFailed {
            message: format!("Failed to initialize grep.app HTTP client: {e}"),
        })?;

    let response = client
        .post(GREP_APP_MCP_URL)
        .header("accept", "application/json, text/event-stream")
        .header("content-type", "application/json")
        .json(&request)
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                ToolError::ExecutionFailed {
                    message: "grep.app search request timed out".to_string(),
                }
            } else {
                ToolError::ExecutionFailed {
                    message: format!("grep.app search request failed: {e}"),
                }
            }
        })?;

    if !response.status().is_success() {
        return Err(ToolError::ExecutionFailed {
            message: format!("grep.app search returned HTTP {}", response.status()),
        });
    }

    let response_text = response.text().await.map_err(|e| ToolError::ExecutionFailed {
        message: format!("Failed to read grep.app response body: {e}"),
    })?;

    parse_mcp_sse_response(&response_text, "grep.app")
}

fn parse_mcp_sse_response(response_text: &str, provider: &str) -> Result<String, ToolError> {
    let mut last_error: Option<String> = None;
    let mut found_data_line = false;

    for line in response_text.split('\n') {
        if !line.starts_with("data: ") {
            continue;
        }

        let payload = line.trim_start_matches("data: ").trim();
        if payload.is_empty() || payload == "[DONE]" {
            continue;
        }

        found_data_line = true;

        let parsed: ExaMcpResponse = match serde_json::from_str(payload) {
            Ok(p) => p,
            Err(_) => {
                return Err(ToolError::ExecutionFailed {
                    message: payload.to_string(),
                });
            }
        };

        let result = parsed.result.ok_or_else(|| ToolError::ExecutionFailed {
            message: format!("{provider} response missing result payload"),
        })?;

        let first = result.content.first().ok_or_else(|| ToolError::ExecutionFailed {
            message: format!("{provider} response contained no content"),
        })?;

        if first.content_type != "text" {
            last_error = Some(format!(
                "Unexpected {provider} content type '{}' in SSE response",
                first.content_type
            ));
            continue;
        }

        if first.text.trim().is_empty() {
            return Err(ToolError::ExecutionFailed {
                message: format!("{provider} response contained no content"),
            });
        }

        return Ok(first.text.clone());
    }

    if let Some(message) = last_error {
        return Err(ToolError::ExecutionFailed { message });
    }

    if !found_data_line {
        let trimmed = response_text.trim();
        if !trimmed.is_empty() {
            return Err(ToolError::ExecutionFailed {
                message: trimmed.to_string(),
            });
        }
    }

    Err(ToolError::ExecutionFailed {
        message: format!("{provider} returned an empty response"),
    })
}

async fn exa_search(
    query: &str,
    num_results: usize,
    search_type: &str,
    livecrawl: &str,
    context_max_chars: Option<usize>,
) -> Result<String, ToolError> {
    let mut args = serde_json::json!({
        "query": query,
        "numResults": num_results,
        "type": search_type,
        "livecrawl": livecrawl,
    });
    if let Some(max_chars) = context_max_chars {
        args["contextMaxCharacters"] = serde_json::json!(max_chars);
    }

    let request = ExaMcpRequest {
        jsonrpc: "2.0".to_string(),
        id: 1,
        method: "tools/call".to_string(),
        params: ExaMcpParams {
            name: "web_search_exa".to_string(),
            arguments: args,
        },
    };

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(EXA_SEARCH_TIMEOUT_SECS))
        .user_agent("uira/0.1 (+https://github.com/junhoyeo/uira)")
        .build()
        .map_err(|e| ToolError::ExecutionFailed {
            message: format!("Failed to initialize Exa HTTP client: {e}"),
        })?;

    let response = client
        .post(EXA_MCP_URL)
        .header("accept", "application/json, text/event-stream")
        .header("content-type", "application/json")
        .json(&request)
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                ToolError::ExecutionFailed {
                    message: "Exa web search request timed out".to_string(),
                }
            } else {
                ToolError::ExecutionFailed {
                    message: format!("Exa web search request failed: {e}"),
                }
            }
        })?;

    if !response.status().is_success() {
        return Err(ToolError::ExecutionFailed {
            message: format!("Exa web search returned HTTP {}", response.status()),
        });
    }

    let response_text = response.text().await.map_err(|e| ToolError::ExecutionFailed {
        message: format!("Failed to read Exa web search response body: {e}"),
    })?;

    match parse_mcp_sse_response(&response_text, "Exa") {
        Ok(text) => Ok(text),
        Err(ToolError::ExecutionFailed { message })
            if message == "Exa response contained no content" =>
        {
            Ok("No search results found".to_string())
        }
        Err(e) => Err(e),
    }
}

async fn exa_code_search(query: &str, tokens_num: usize) -> Result<String, ToolError> {
    let request = ExaMcpRequest {
        jsonrpc: "2.0".to_string(),
        id: 1,
        method: "tools/call".to_string(),
        params: ExaMcpParams {
            name: "get_code_context_exa".to_string(),
            arguments: serde_json::json!({
                "query": query,
                "tokensNum": tokens_num,
            }),
        },
    };

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(EXA_CODE_SEARCH_TIMEOUT_SECS))
        .user_agent("uira/0.1 (+https://github.com/junhoyeo/uira)")
        .build()
        .map_err(|e| ToolError::ExecutionFailed {
            message: format!("Failed to initialize Exa HTTP client: {e}"),
        })?;

    let response = client
        .post(EXA_MCP_URL)
        .header("accept", "application/json, text/event-stream")
        .header("content-type", "application/json")
        .json(&request)
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                ToolError::ExecutionFailed {
                    message: "Exa code search request timed out".to_string(),
                }
            } else {
                ToolError::ExecutionFailed {
                    message: format!("Exa code search request failed: {e}"),
                }
            }
        })?;

    if !response.status().is_success() {
        return Err(ToolError::ExecutionFailed {
            message: format!("Exa code search returned HTTP {}", response.status()),
        });
    }

    let response_text = response.text().await.map_err(|e| ToolError::ExecutionFailed {
        message: format!("Failed to read Exa code search response body: {e}"),
    })?;

    parse_mcp_sse_response(&response_text, "Exa")
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
        ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        let runtime = load_runtime_config(&ctx.cwd);
        if !runtime.enabled {
            return Err(ToolError::ExecutionFailed {
                message: "fetch_url is disabled by uira.yml tools.web_search.enabled=false"
                    .to_string(),
            });
        }

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
        validate_fetch_url(&url).await?;

        {
            let mut state = STATE.lock().await;
            state.cleanup(runtime.rate_limit_window_secs);
            state.check_rate_limit(
                runtime.rate_limit_max_requests,
                runtime.rate_limit_window_secs,
            )?;
            state.request_times.push_back(Instant::now());
        }

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::none())
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

        if let Some(len) = response.content_length() {
            if len as usize > FETCH_MAX_RESPONSE_BYTES {
                return Err(ToolError::ExecutionFailed {
                    message: format!(
                        "Response too large: {} bytes exceeds limit of {} bytes",
                        len, FETCH_MAX_RESPONSE_BYTES
                    ),
                });
            }
        }

        let mut stream = response.bytes_stream();
        let mut body_bytes = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| ToolError::ExecutionFailed {
                message: format!("Failed to read response body: {e}"),
            })?;
            body_bytes.extend_from_slice(&chunk);
            if body_bytes.len() > FETCH_MAX_RESPONSE_BYTES {
                return Err(ToolError::ExecutionFailed {
                    message: format!(
                        "Response too large: exceeded limit of {} bytes",
                        FETCH_MAX_RESPONSE_BYTES
                    ),
                });
            }
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
        serde_json::to_value(out)
            .map(ToolOutput::json)
            .map_err(|e| ToolError::ExecutionFailed {
                message: format!("Failed to serialize output: {}", e),
            })
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

fn load_runtime_config(cwd: &Path) -> RuntimeConfig {
    let config_path = cwd.join("uira.yml");
    let content = match std::fs::read_to_string(&config_path) {
        Ok(content) => content,
        Err(_) => return RuntimeConfig::default(),
    };

    let parsed: UiraConfigFile = match serde_yaml_ng::from_str(&content) {
        Ok(parsed) => parsed,
        Err(_) => return RuntimeConfig::default(),
    };

    let mut runtime = RuntimeConfig::default();
    if let Some(web_search) = parsed.tools.and_then(|t| t.web_search) {
        if let Some(enabled) = web_search.enabled {
            runtime.enabled = enabled;
        }
        if let Some(provider) = web_search.provider {
            runtime.provider = provider;
        }
        if let Some(cache_ttl) = web_search.cache_ttl {
            runtime.cache_ttl_secs = cache_ttl;
        }
        if let Some(max_requests) = web_search.rate_limit_max_requests {
            runtime.rate_limit_max_requests = max_requests.max(1);
        }
        if let Some(window_secs) = web_search.rate_limit_window_secs {
            runtime.rate_limit_window_secs = window_secs.max(1);
        }
    }

    runtime
}

async fn validate_fetch_url(url: &reqwest::Url) -> Result<(), ToolError> {
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
            return Err(ToolError::InvalidInput {
                message: "Blocked private or local IP address for fetch_url".to_string(),
            });
        }
    } else {
        let port = url.port_or_known_default().unwrap_or(80);
        let resolved = tokio::net::lookup_host((host.as_str(), port))
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                message: format!("Failed to resolve host '{host}': {e}"),
            })?;

        for addr in resolved {
            if is_private_or_local_ip(addr.ip()) {
                return Err(ToolError::InvalidInput {
                    message: "Blocked hostname resolving to private or local address".to_string(),
                });
            }
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
            if let Some(v4_mapped) = v6.to_ipv4_mapped() {
                return v4_mapped.is_private()
                    || v4_mapped.is_loopback()
                    || v4_mapped.is_link_local()
                    || v4_mapped.is_multicast()
                    || v4_mapped == Ipv4Addr::new(169, 254, 169, 254)
                    || v4_mapped == Ipv4Addr::new(0, 0, 0, 0);
            }

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

    #[tokio::test]
    async fn blocks_localhost_urls() {
        let url = reqwest::Url::parse("http://localhost:8080").unwrap();
        assert!(validate_fetch_url(&url).await.is_err());
    }

    #[tokio::test]
    async fn blocks_private_ip_urls() {
        let url = reqwest::Url::parse("http://127.0.0.1/api").unwrap();
        assert!(validate_fetch_url(&url).await.is_err());
    }

    #[tokio::test]
    async fn allows_public_https_urls() {
        let url = reqwest::Url::parse("https://example.com/docs").unwrap();
        assert!(validate_fetch_url(&url).await.is_ok());
    }
}
