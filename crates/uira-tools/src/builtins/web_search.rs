//! Web search tool for live documentation lookups

use async_trait::async_trait;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use uira_protocol::{ApprovalRequirement, JsonSchema, SandboxPreference, ToolOutput};

use crate::{Tool, ToolContext, ToolError};

const DEFAULT_LIMIT: usize = 5;
const MAX_LIMIT: usize = 10;
const CACHE_TTL: Duration = Duration::from_secs(300);
const MIN_REQUEST_INTERVAL: Duration = Duration::from_millis(500);

static HTTP_CLIENT: Lazy<reqwest::Client> = Lazy::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent("uira-web-search/0.1")
        .build()
        .expect("failed to build web search HTTP client")
});

static SEARCH_CACHE: Lazy<Mutex<HashMap<String, CacheEntry>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

static LAST_REQUEST_AT: Lazy<Mutex<Option<Instant>>> = Lazy::new(|| Mutex::new(None));

#[derive(Debug, Deserialize)]
struct WebSearchInput {
    query: String,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    mode: Option<SearchMode>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum SearchMode {
    #[default]
    Fast,
    Live,
}

#[derive(Debug, Clone, Serialize)]
struct WebSearchResult {
    title: String,
    url: String,
    snippet: String,
}

#[derive(Debug, Clone, Serialize)]
struct WebSearchResponse {
    query: String,
    provider: &'static str,
    mode: SearchMode,
    cached: bool,
    results: Vec<WebSearchResult>,
}

#[derive(Clone)]
struct CacheEntry {
    output: WebSearchResponse,
    expires_at: Instant,
}

#[derive(Debug, Deserialize)]
struct DuckDuckGoResponse {
    #[serde(rename = "Heading", default)]
    heading: String,
    #[serde(rename = "AbstractText", default)]
    abstract_text: String,
    #[serde(rename = "AbstractURL", default)]
    abstract_url: String,
    #[serde(rename = "Results", default)]
    results: Vec<DuckDuckGoTopic>,
    #[serde(rename = "RelatedTopics", default)]
    related_topics: Vec<DuckDuckGoTopic>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum DuckDuckGoTopic {
    Item {
        #[serde(rename = "Text")]
        text: String,
        #[serde(rename = "FirstURL")]
        first_url: String,
    },
    Group {
        #[serde(rename = "Topics", default)]
        topics: Vec<DuckDuckGoTopic>,
    },
}

pub struct WebSearchTool;

impl WebSearchTool {
    pub fn new() -> Self {
        Self
    }

    fn normalize_limit(limit: Option<usize>) -> usize {
        limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
    }

    async fn read_cache(cache_key: &str) -> Option<WebSearchResponse> {
        let now = Instant::now();
        let mut cache = SEARCH_CACHE.lock().await;
        cache.retain(|_, entry| entry.expires_at > now);

        cache.get(cache_key).map(|entry| {
            let mut output = entry.output.clone();
            output.cached = true;
            output
        })
    }

    async fn write_cache(cache_key: String, output: WebSearchResponse) {
        let entry = CacheEntry {
            output,
            expires_at: Instant::now() + CACHE_TTL,
        };
        SEARCH_CACHE.lock().await.insert(cache_key, entry);
    }

    async fn wait_for_rate_limit() {
        let delay = {
            let mut last_request = LAST_REQUEST_AT.lock().await;
            let now = Instant::now();

            let wait = last_request
                .map(|last| {
                    MIN_REQUEST_INTERVAL.saturating_sub(now.saturating_duration_since(last))
                })
                .unwrap_or_default();

            *last_request = Some(now + wait);
            wait
        };

        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
    }

    async fn fetch_duckduckgo(query: &str) -> Result<DuckDuckGoResponse, ToolError> {
        let response = HTTP_CLIENT
            .get("https://api.duckduckgo.com/")
            .query(&[
                ("q", query),
                ("format", "json"),
                ("no_html", "1"),
                ("no_redirect", "1"),
                ("skip_disambig", "1"),
            ])
            .send()
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                message: format!("Web search request failed: {}", e),
            })?;

        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read response body".to_string());
            return Err(ToolError::ExecutionFailed {
                message: format!("Web search provider returned {}: {}", status, body),
            });
        }

        response
            .json::<DuckDuckGoResponse>()
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                message: format!("Failed to parse web search response: {}", e),
            })
    }

    fn collect_topics(topics: &[DuckDuckGoTopic], output: &mut Vec<(String, String)>) {
        for topic in topics {
            match topic {
                DuckDuckGoTopic::Item { text, first_url } => {
                    output.push((text.clone(), first_url.clone()));
                }
                DuckDuckGoTopic::Group { topics } => Self::collect_topics(topics, output),
            }
        }
    }

    fn split_title_and_snippet(text: &str) -> (String, String) {
        let trimmed = text.trim();
        if let Some((title, snippet)) = trimmed.split_once(" - ") {
            (title.trim().to_string(), snippet.trim().to_string())
        } else {
            (trimmed.to_string(), trimmed.to_string())
        }
    }

    fn push_result(
        results: &mut Vec<WebSearchResult>,
        seen_urls: &mut HashSet<String>,
        title: String,
        url: String,
        snippet: String,
    ) {
        let title = title.trim().to_string();
        let url = url.trim().to_string();
        let snippet = snippet.trim().to_string();

        if title.is_empty() || url.is_empty() || snippet.is_empty() {
            return;
        }

        if seen_urls.insert(url.clone()) {
            results.push(WebSearchResult {
                title,
                url,
                snippet,
            });
        }
    }

    fn build_results(
        query: &str,
        response: DuckDuckGoResponse,
        limit: usize,
    ) -> Vec<WebSearchResult> {
        let mut results = Vec::new();
        let mut seen_urls = HashSet::new();

        if !response.abstract_text.trim().is_empty() && !response.abstract_url.trim().is_empty() {
            let title = if response.heading.trim().is_empty() {
                query.to_string()
            } else {
                response.heading.trim().to_string()
            };

            Self::push_result(
                &mut results,
                &mut seen_urls,
                title,
                response.abstract_url,
                response.abstract_text,
            );
        }

        if results.len() >= limit {
            return results;
        }

        let mut topics = Vec::new();
        Self::collect_topics(&response.results, &mut topics);
        Self::collect_topics(&response.related_topics, &mut topics);

        for (text, url) in topics {
            if results.len() >= limit {
                break;
            }

            let (title, snippet) = Self::split_title_and_snippet(&text);
            Self::push_result(&mut results, &mut seen_urls, title, url, snippet);
        }

        results
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
        "Search the web for up-to-date documentation and return relevant snippets."
    }

    fn schema(&self) -> JsonSchema {
        JsonSchema::object()
            .property(
                "query",
                JsonSchema::string().description("Search query string"),
            )
            .property(
                "limit",
                JsonSchema::number().description("Maximum number of results (default: 5, max: 10)"),
            )
            .property(
                "mode",
                JsonSchema::string()
                    .description("Search mode: 'fast' (cached) or 'live' (real-time request)"),
            )
            .required(&["query"])
    }

    fn approval_requirement(&self, _input: &serde_json::Value) -> ApprovalRequirement {
        ApprovalRequirement::Skip {
            bypass_sandbox: false,
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

        let query = input.query.trim().to_string();
        if query.is_empty() {
            return Err(ToolError::InvalidInput {
                message: "query must not be empty".to_string(),
            });
        }

        let limit = Self::normalize_limit(input.limit);
        let mode = input.mode.unwrap_or_default();
        let cache_key = format!("{}:{}", query.to_lowercase(), limit);

        if mode == SearchMode::Fast {
            if let Some(cached_output) = Self::read_cache(&cache_key).await {
                return Ok(ToolOutput::json(
                    serde_json::to_value(cached_output).unwrap(),
                ));
            }
        }

        Self::wait_for_rate_limit().await;

        let provider_response = Self::fetch_duckduckgo(&query).await?;
        let results = Self::build_results(&query, provider_response, limit);

        let output = WebSearchResponse {
            query,
            provider: "duckduckgo",
            mode,
            cached: false,
            results,
        };

        if mode == SearchMode::Fast {
            Self::write_cache(cache_key, output.clone()).await;
        }

        Ok(ToolOutput::json(serde_json::to_value(output).unwrap()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn split_title_and_snippet_handles_standard_format() {
        let (title, snippet) =
            WebSearchTool::split_title_and_snippet("Rust async - Learn async Rust patterns");
        assert_eq!(title, "Rust async");
        assert_eq!(snippet, "Learn async Rust patterns");
    }

    #[test]
    fn build_results_extracts_nested_topics() {
        let response: DuckDuckGoResponse = serde_json::from_value(json!({
            "Heading": "Rust",
            "AbstractText": "Rust is a systems programming language.",
            "AbstractURL": "https://www.rust-lang.org/",
            "Results": [],
            "RelatedTopics": [
                {
                    "Text": "Tokio - An asynchronous runtime for Rust",
                    "FirstURL": "https://tokio.rs/"
                },
                {
                    "Name": "Documentation",
                    "Topics": [
                        {
                            "Text": "The Rust Book - Official Rust guide",
                            "FirstURL": "https://doc.rust-lang.org/book/"
                        }
                    ]
                }
            ]
        }))
        .unwrap();

        let results = WebSearchTool::build_results("rust", response, 3);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].title, "Rust");
        assert_eq!(results[1].title, "Tokio");
        assert_eq!(results[2].title, "The Rust Book");
    }

    #[tokio::test]
    async fn execute_rejects_empty_query() {
        let tool = WebSearchTool::new();
        let ctx = ToolContext::default();

        let err = tool
            .execute(json!({"query": "   "}), &ctx)
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidInput { .. }));
    }
}
