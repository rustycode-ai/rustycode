//! Web search tool for general web queries
//!
//! This tool provides web search capabilities using multiple FREE APIs:
//! - Wikipedia API (factual questions, current events)
//! - DuckDuckGo (general queries, instant answers)
//! - Exa Search (if API key configured, premium results)

use crate::{ToolOutput, ToolPermission, ToolTag};
use anyhow::{anyhow, Result};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use std::env;

#[derive(Deserialize, JsonSchema)]
pub struct WebSearchParams {
    /// Search query. Use specific, factual questions for best results.
    query: String,
    /// Maximum number of results to return (default: 5)
    num_results: Option<u64>,
    /// Preferred search source (default: 'auto')
    source: Option<String>,
}

rustycode_tools_api::define_tool! {
    pub struct WebSearchTool;

    name: "WebSearch",
    description: r#"Search the web for current information and factual queries.

Use this tool when you need to:
- Find current events or recent news
- Look up factual information (people, places, things)
- Get up-to-date data beyond training cutoff
- Answer questions about recent developments

No API key required for basic functionality!"#,
    permission: ToolPermission::Network,
    tags: [ToolTag::Explore],

    execute(params: WebSearchParams, _ctx) {
        let query = &params.query;
        let num_results = params.num_results.unwrap_or(5).clamp(1, 10) as usize;
        let source = params.source.as_deref().unwrap_or("auto");
        let exa_api_key = env::var("EXA_API_KEY").ok();

        let results = match source {
            "wikipedia" => search_wikipedia(query, num_results)?,
            "news" => {
                if let Some(ref key) = exa_api_key {
                    search_exa_news(query, num_results, key)?
                } else {
                    search_duckduckgo(query, num_results)?
                }
            }
            "web" | "auto" => {
                if source == "auto" && is_factual_query(query) {
                    match search_wikipedia(query, num_results) {
                        Ok(wiki_results) => wiki_results,
                        Err(_) => match search_duckduckgo(query, num_results) {
                            Ok(ddg_results) => ddg_results,
                            Err(_) => search_fallback(query, num_results, "web")?,
                        },
                    }
                } else if let Some(ref key) = exa_api_key {
                    match search_exa_web(query, num_results, key) {
                        Ok(exa_results) => exa_results,
                        Err(_) => search_duckduckgo(query, num_results)?,
                    }
                } else {
                    match search_duckduckgo(query, num_results) {
                        Ok(ddg_results) => ddg_results,
                        Err(_) => search_fallback(query, num_results, "web")?,
                    }
                }
            }
            _ => search_duckduckgo(query, num_results)
                .or_else(|_| search_fallback(query, num_results, "web"))?,
        };

        Ok(ToolOutput::text(results))
    }
}

fn is_factual_query(query: &str) -> bool {
    let query_lower = query.to_lowercase();
    let factual_patterns = [
        "who is",
        "who was",
        "what is",
        "prime minister",
        "president",
        "ceo",
        "founder",
        "born",
        "died",
        "biography",
        "history of",
        "when did",
        "where is",
        "capital of",
    ];
    factual_patterns.iter().any(|p| query_lower.contains(p))
}

// --- Wikipedia ---

fn search_wikipedia(query: &str, num_results: usize) -> Result<String> {
    let q = query.to_string();
    rustycode_shared_runtime::block_on_shared(search_wikipedia_async(&q, num_results))
}

async fn search_wikipedia_async(query: &str, num_results: usize) -> Result<String> {
    let search_url = format!(
        "https://en.wikipedia.org/w/api.php?action=opensearch&search={}&limit={}&namespace=0&format=json",
        urlencoding::encode(query),
        num_results
    );
    let (status, json_response): (u16, Value) =
        super::client::http_get_json(&search_url, 15).await?;
    if status != 200 {
        return Err(anyhow!("Wikipedia search failed: HTTP {status}"));
    }
    let arr = json_response
        .as_array()
        .ok_or_else(|| anyhow!("Invalid Wikipedia response"))?;
    if arr.len() < 4 {
        return Ok(format!("No Wikipedia results found for '{query}'."));
    }
    let titles = arr
        .get(1)
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("Missing titles"))?;
    let descriptions = arr
        .get(2)
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("Missing descriptions"))?;
    let urls = arr
        .get(3)
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("Missing URLs"))?;
    if titles.is_empty() {
        return Ok(format!("No Wikipedia results found for '{query}'."));
    }
    let mut output = format!("**Wikipedia Results for '{query}'**\n\n");
    #[allow(clippy::needless_range_loop)]
    for idx in 0..num_results.min(titles.len()) {
        let title = titles[idx].as_str().unwrap_or("Unknown");
        let desc = descriptions.get(idx).and_then(|v| v.as_str()).unwrap_or("");
        let url = urls.get(idx).and_then(|v| v.as_str()).unwrap_or("");
        output.push_str(&format!(
            "{}. **{}**\n   {}\n   {}\n\n",
            idx + 1,
            title,
            truncate_text(desc, 200),
            url
        ));
    }
    Ok(output)
}

// --- DuckDuckGo ---

fn search_duckduckgo(query: &str, num_results: usize) -> Result<String> {
    let q = query.to_string();
    rustycode_shared_runtime::block_on_shared(search_duckduckgo_async(&q, num_results))
}

async fn search_duckduckgo_async(query: &str, num_results: usize) -> Result<String> {
    let url = format!(
        "https://api.duckduckgo.com/?q={}&format=json&no_html=1&skip_disambig=0",
        urlencoding::encode(query)
    );
    let (_status, json_response): (u16, Value) = super::client::http_get_json(&url, 15).await?;
    let mut output = format!("**DuckDuckGo Results for '{query}'**\n\n");
    let mut has_results = false;

    if let Some(heading) = json_response.get("Heading").and_then(|v| v.as_str()) {
        if !heading.is_empty() {
            let abs_url = json_response
                .get("AbstractURL")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let abs_src = json_response
                .get("AbstractSource")
                .and_then(|v| v.as_str())
                .unwrap_or("Wikipedia");
            output.push_str(&format!("**{heading}**\nSource: {abs_src}\n{abs_url}\n\n"));
            has_results = true;
        }
    }
    if let Some(text) = json_response.get("AbstractText").and_then(|v| v.as_str()) {
        if !text.is_empty() {
            output.push_str(&format!("**Summary**\n{}\n\n", truncate_text(text, 500)));
            has_results = true;
        }
    }
    if let Some(answer) = json_response.get("Answer").and_then(|v| v.as_str()) {
        if !answer.is_empty() {
            output.push_str(&format!("**Answer**\n{}\n\n", truncate_text(answer, 300)));
            has_results = true;
        }
    }
    if let Some(infobox) = json_response.get("Infobox").and_then(|v| v.as_object()) {
        if let Some(content) = infobox.get("content").and_then(|v| v.as_str()) {
            if !content.is_empty() {
                output.push_str(&format!(
                    "**Quick Facts**\n{}\n\n",
                    truncate_text(content, 300)
                ));
                has_results = true;
            }
        }
    }
    if let Some(topics) = json_response
        .get("RelatedTopics")
        .and_then(|v| v.as_array())
    {
        let mut count = 0;
        for topic in topics.iter().take(num_results) {
            if topic.get("Topics").is_some() {
                continue;
            }
            if let Some(text) = topic.get("Text").and_then(|v| v.as_str()) {
                if !text.is_empty() {
                    let url = topic.get("FirstURL").and_then(|v| v.as_str()).unwrap_or("");
                    output.push_str(&format!(
                        "{}. **{}**\n   {}\n\n",
                        count + 1,
                        truncate_text(text, 200),
                        url
                    ));
                    count += 1;
                    has_results = true;
                    if count >= num_results {
                        break;
                    }
                }
            }
        }
    }
    if !has_results {
        output
            .push_str("(No instant answers found. Try source: \"wikipedia\" or set EXA_API_KEY)\n");
    }
    Ok(output)
}

// --- Exa Web ---

fn search_exa_web(query: &str, num_results: usize, api_key: &str) -> Result<String> {
    let q = query.to_string();
    let k = api_key.to_string();
    rustycode_shared_runtime::block_on_shared(search_exa_web_async(&q, num_results, &k))
}

async fn search_exa_web_async(query: &str, num_results: usize, api_key: &str) -> Result<String> {
    let body = json!({ "query": query, "numResults": num_results, "contents": { "text": true } });
    let client = super::client::build_client(std::time::Duration::from_secs(15))?;
    let response = client
        .post("https://api.exa.ai/search")
        .header("x-api-key", api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| anyhow!("Exa API call failed: {e}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let err = response
            .text()
            .await
            .unwrap_or_else(|_| "unable to read error".into());
        return Err(anyhow!("Exa API error: {status} - {err}"));
    }
    let results_json: Value = response.json().await?;
    let results = results_json
        .get("results")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("Invalid Exa response"))?;
    if results.is_empty() {
        return Ok(format!("No Exa results found for '{query}'"));
    }

    let mut output = format!("**Web Search Results for '{query}'**\n\n");
    for (idx, r) in results.iter().take(num_results).enumerate() {
        let title = r
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Untitled");
        let url = r.get("url").and_then(|v| v.as_str()).unwrap_or("");
        let snippet = r
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("No snippet");
        output.push_str(&format!(
            "{}. **{}**\n   {}\n   {}\n\n",
            idx + 1,
            title,
            truncate_text(snippet, 300),
            url
        ));
    }
    Ok(output)
}

// --- Exa News ---

fn search_exa_news(query: &str, num_results: usize, api_key: &str) -> Result<String> {
    let q = query.to_string();
    let k = api_key.to_string();
    rustycode_shared_runtime::block_on_shared(search_exa_news_async(&q, num_results, &k))
}

async fn search_exa_news_async(query: &str, num_results: usize, api_key: &str) -> Result<String> {
    let body = json!({ "query": query, "numResults": num_results, "useAutoprompt": true, "category": "news", "contents": { "text": true } });
    let client = super::client::build_client(std::time::Duration::from_secs(15))?;
    let response = client
        .post("https://api.exa.ai/search")
        .header("x-api-key", api_key)
        .json(&body)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let err = response
            .text()
            .await
            .unwrap_or_else(|_| "unable to read error".into());
        return Err(anyhow!("Exa News API error: {status} - {err}"));
    }
    let results_json: Value = response.json().await?;
    let results = results_json
        .get("results")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("Invalid Exa News response"))?;

    let mut output = format!("**News Results for '{query}'**\n\n");
    for (idx, r) in results.iter().take(num_results).enumerate() {
        let title = r
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Untitled");
        let url = r.get("url").and_then(|v| v.as_str()).unwrap_or("");
        let snippet = r
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("No snippet");
        let date = r
            .get("publishedDate")
            .and_then(|v| v.as_str())
            .unwrap_or("Recent");
        output.push_str(&format!(
            "{}. **{}** ({})\n   {}\n   {}\n\n",
            idx + 1,
            title,
            date,
            truncate_text(snippet, 250),
            url
        ));
    }
    Ok(output)
}

// --- URL generators ---

fn search_fallback(query: &str, _num_results: usize, source: &str) -> Result<String> {
    let mut output = format!(
        "**Web Search: '{query}'**\n\nSet EXA_API_KEY for automatic results.\n\n**Manual Search Links:**\n\n"
    );
    match source {
        "news" => output.push_str(&format!(
            "- [Google News]({})\n- [DuckDuckGo News]({})\n",
            google_news_url(query),
            duckduckgo_news_url(query)
        )),
        _ => output.push_str(&format!(
            "- [Wikipedia]({})\n- [Google]({})\n- [DuckDuckGo]({})\n",
            wikipedia_search_url(query),
            google_search_url(query),
            duckduckgo_search_url(query)
        )),
    }
    Ok(output)
}

fn wikipedia_search_url(q: &str) -> String {
    format!(
        "https://en.wikipedia.org/w/index.php?search={}",
        urlencoding::encode(q)
    )
}
fn google_search_url(q: &str) -> String {
    format!("https://www.google.com/search?q={}", urlencoding::encode(q))
}
fn duckduckgo_search_url(q: &str) -> String {
    format!("https://duckduckgo.com/?q={}", urlencoding::encode(q))
}
fn google_news_url(q: &str) -> String {
    format!(
        "https://news.google.com/search?q={}",
        urlencoding::encode(q)
    )
}
fn duckduckgo_news_url(q: &str) -> String {
    format!("https://duckduckgo.com/?q=!news {}", urlencoding::encode(q))
}

fn truncate_text(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        return text.to_string();
    }
    let mut end = max_len;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    let truncated = &text[..end];
    if let Some(last_space) = truncated.rfind(' ') {
        format!("{}...", &truncated[..last_space])
    } else {
        format!("{truncated}...")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Tool;

    #[test]
    fn test_web_search_tool_metadata() {
        let tool = WebSearchTool;
        assert_eq!(tool.name(), "WebSearch");
        assert!(tool.description().contains("current information"));
        assert_eq!(tool.permission(), ToolPermission::Network);
    }

    #[test]
    fn test_is_factual_query() {
        assert!(is_factual_query("Who is the Prime Minister of Thailand"));
        assert!(is_factual_query("biography of Elon Musk"));
        assert!(!is_factual_query("how to parse JSON in Rust"));
    }

    #[test]
    fn test_search_urls() {
        assert!(wikipedia_search_url("test").contains("wikipedia.org"));
        assert!(google_search_url("test").contains("google.com"));
    }

    #[test]
    fn test_truncate_text() {
        let truncated = truncate_text("This is a very long text that should be truncated", 30);
        assert!(truncated.len() <= 33);
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn test_truncate_text_handles_multibyte_boundary() {
        let text = "é".repeat(20);
        let truncated = truncate_text(&text, 7);
        assert!(truncated.is_char_boundary(truncated.len()));
        assert!(truncated.ends_with("..."));
    }
}
