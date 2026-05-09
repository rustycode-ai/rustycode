//! Web search tool for general web queries
//!
//! This tool provides web search capabilities using multiple FREE APIs:
//! - Wikipedia API (factual questions, current events)
//! - `DuckDuckGo` (general queries, instant answers)
//! - Exa Search (if API key configured, premium results)
//!
//! No API key required for basic functionality!

const USER_AGENT: &str = concat!("RustyCode/", env!("CARGO_PKG_VERSION"));

use crate::{ToolOutput, ToolPermission, ToolTag};
use anyhow::{anyhow, Result};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use std::env;

/// Parameters for the web search tool.
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

    name: "web_search",
    description: r#"Search the web for current information and factual queries.

Use this tool when you need to:
- Find current events or recent news
- Look up factual information (people, places, things)
- Get up-to-date data beyond training cutoff
- Answer questions about recent developments

**Examples:**
- "current Prime Minister of Thailand"
- "latest Python version 2026"
- "Tesla stock price today"
- "who won the Super Bowl 2026"

The tool searches using multiple FREE sources:
- Wikipedia API (factual information, biographies) - NO API key needed
- DuckDuckGo Instant Answer (general queries, definitions) - NO API key needed
- Exa Search (premium results, optional - requires EXA_API_KEY env var)

No API key required for basic functionality!"#,
    permission: ToolPermission::Network,
    tags: [ToolTag::Explore],
    defer_loading: true,

    execute(params: WebSearchParams, _ctx) {
        let query = &params.query;
        let num_results = params.num_results.unwrap_or(5).clamp(1, 10) as usize;
        let source = params.source.as_deref().unwrap_or("auto");

        // Check for Exa API key (optional, for premium results)
        let exa_api_key = env::var("EXA_API_KEY").ok();

        // Route to appropriate search method
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
                // For auto mode, try multiple sources in order
                if source == "auto" && is_factual_query(query) {
                    // Try Wikipedia first for factual queries
                    match search_wikipedia(query, num_results) {
                        Ok(wiki_results) => wiki_results,
                        Err(_) => {
                            // Fall back to DuckDuckGo
                            match search_duckduckgo(query, num_results) {
                                Ok(ddg_results) => ddg_results,
                                Err(_) => {
                                    // Final fallback to URLs
                                    search_fallback(query, num_results, "web")?
                                }
                            }
                        }
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

/// Check if query is factual/biographical (good for Wikipedia)
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

    factual_patterns
        .iter()
        .any(|pattern| query_lower.contains(pattern))
}

/// Search Wikipedia (FREE, no API key needed)
fn search_wikipedia(query: &str, num_results: usize) -> Result<String> {
    use reqwest::blocking::Client;

    let client = Client::new();

    // Wikipedia API: https://en.wikipedia.org/w/api.php
    // Action: opensearch (search for pages)
    // Format: json
    let search_url = format!(
        "https://en.wikipedia.org/w/api.php?action=opensearch&search={}&limit={}&namespace=0&format=json",
        urlencoding::encode(query),
        num_results
    );

    let response = client
        .get(&search_url)
        .header("User-Agent", USER_AGENT)
        .send()?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "Wikipedia search failed: HTTP {}",
            response.status()
        ));
    }

    let json_response: Value = response.json()?;

    // Wikipedia opensearch returns an array:
    // [query, [title1, title2, ...], [description1, description2, ...], [url1, url2, ...]]
    let results_array = json_response
        .as_array()
        .ok_or_else(|| anyhow!("Invalid Wikipedia API response format"))?;

    if results_array.len() < 4 {
        return Ok(format!(
            "No Wikipedia results found for '{query}'.\n\n\
             Try:\n\
             - Rephrasing your query\n\
             - Using source: 'web' for broader search\n\
             - Checking spelling"
        ));
    }

    let titles = results_array
        .get(1)
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("Missing titles in Wikipedia response"))?;

    let descriptions = results_array
        .get(2)
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("Missing descriptions in Wikipedia response"))?;

    let urls = results_array
        .get(3)
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("Missing URLs in Wikipedia response"))?;

    if titles.is_empty() {
        return Ok(format!(
            "No Wikipedia results found for '{query}'.\n\n\
             Try:\n\
             - Rephrasing your query\n\
             - Using source: 'web' for broader search\n\
             - Checking spelling"
        ));
    }

    let mut output = String::new();
    output.push_str(&format!("**Wikipedia Results for '{query}'**\n\n"));

    #[allow(clippy::needless_range_loop)]
    for idx in 0..num_results.min(titles.len()) {
        let title = titles[idx].as_str().unwrap_or("Unknown");

        let description = descriptions
            .get(idx)
            .and_then(|v| v.as_str())
            .unwrap_or("No description available");

        let url = urls.get(idx).and_then(|v| v.as_str()).unwrap_or("");

        output.push_str(&format!(
            "{}. **{}**\n   {}\n   {}\n\n",
            idx + 1,
            title,
            truncate_text(description, 200),
            url
        ));
    }

    Ok(output)
}

/// Search `DuckDuckGo` Instant Answer API (FREE, no API key needed)
fn search_duckduckgo(query: &str, num_results: usize) -> Result<String> {
    use reqwest::blocking::Client;

    let client = Client::new();

    // DuckDuckGo Instant Answer API
    let url = format!(
        "https://api.duckduckgo.com/?q={}&format=json&no_html=1&skip_disambig=0",
        urlencoding::encode(query)
    );

    let response = client.get(&url).header("User-Agent", USER_AGENT).send()?;

    if !response.status().is_success() {
        return Err(anyhow!("DuckDuckGo API error: HTTP {}", response.status()));
    }

    let json_response: Value = response.json()?;

    let mut output = String::new();
    let mut has_results = false;

    output.push_str(&format!("**DuckDuckGo Results for '{query}'**\n\n"));

    // Check for heading (topic title)
    if let Some(heading) = json_response.get("Heading").and_then(|v| v.as_str()) {
        if !heading.is_empty() {
            let abstract_url = json_response
                .get("AbstractURL")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let abstract_source = json_response
                .get("AbstractSource")
                .and_then(|v| v.as_str())
                .unwrap_or("Wikipedia");

            output.push_str(&format!(
                "**{heading}**\nSource: {abstract_source}\n{abstract_url}\n\n"
            ));
            has_results = true;
        }
    }

    // Check for instant answer text
    if let Some(abstract_text) = json_response.get("AbstractText").and_then(|v| v.as_str()) {
        if !abstract_text.is_empty() {
            output.push_str(&format!(
                "**Summary**\n{}\n\n",
                truncate_text(abstract_text, 500)
            ));
            has_results = true;
        }
    }

    // Check for Answer field (direct answers)
    if let Some(answer) = json_response.get("Answer").and_then(|v| v.as_str()) {
        if !answer.is_empty() {
            output.push_str(&format!("**Answer**\n{}\n\n", truncate_text(answer, 300)));
            has_results = true;
        }
    }

    // Check for infobox
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

    // Check for related topics
    if let Some(topics) = json_response
        .get("RelatedTopics")
        .and_then(|v| v.as_array())
    {
        let mut count = 0;
        for topic in topics.iter().take(num_results) {
            // Skip topics that are just categories
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

    // If no results, provide helpful message
    if !has_results {
        output.push_str("(No instant answers found for this query. Try:\n");
        output.push_str("  - Using source: \"wikipedia\" for factual queries\n");
        output.push_str("  - Rephrasing your question\n");
        output.push_str("  - Setting EXA_API_KEY for premium search)\n");
    }

    Ok(output)
}

/// Search using Exa API (requires API key, premium results)
fn search_exa_web(query: &str, num_results: usize, api_key: &str) -> Result<String> {
    use reqwest::blocking::Client;

    let client = Client::new();

    let request_body = json!({
        "query": query,
        "numResults": num_results,
        "contents": {
            "text": true
        }
    });

    let response = client
        .post("https://api.exa.ai/search")
        .header("Content-Type", "application/json")
        .header("x-api-key", api_key)
        .json(&request_body)
        .send()
        .map_err(|e| anyhow!("Exa API call failed: {e}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .unwrap_or_else(|_| "unable to read error".to_string());
        return Err(anyhow!("Exa API error: {status} - {error_text}"));
    }

    let results_json: Value = response.json()?;
    let results = results_json
        .get("results")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("Invalid Exa response format"))?;

    if results.is_empty() {
        return Ok(format!("No Exa results found for '{query}'"));
    }

    let mut output = String::new();
    output.push_str(&format!("**Web Search Results for '{query}'**\n\n"));

    for (idx, result) in results.iter().take(num_results).enumerate() {
        let title = result
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Untitled");
        let url = result.get("url").and_then(|v| v.as_str()).unwrap_or("");
        let snippet = result
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("No snippet available");

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

/// Search Exa for news articles
fn search_exa_news(query: &str, num_results: usize, api_key: &str) -> Result<String> {
    use reqwest::blocking::Client;

    let client = Client::new();

    let request_body = json!({
        "query": query,
        "numResults": num_results,
        "useAutoprompt": true,
        "category": "news",
        "contents": {
            "text": true
        }
    });

    let response = client
        .post("https://api.exa.ai/search")
        .header("Content-Type", "application/json")
        .header("x-api-key", api_key)
        .json(&request_body)
        .send()?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .unwrap_or_else(|_| "unable to read error".to_string());
        return Err(anyhow!("Exa News API error: {status} - {error_text}"));
    }

    let results_json: Value = response.json()?;
    let results = results_json
        .get("results")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("Invalid Exa News response"))?;

    let mut output = String::new();
    output.push_str(&format!("**News Results for '{query}'**\n\n"));

    for (idx, result) in results.iter().take(num_results).enumerate() {
        let title = result
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Untitled");
        let url = result.get("url").and_then(|v| v.as_str()).unwrap_or("");
        let snippet = result
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("No snippet");

        let published_date = result
            .get("publishedDate")
            .and_then(|v| v.as_str())
            .unwrap_or("Recent");

        output.push_str(&format!(
            "{}. **{}** ({})\n   {}\n   {}\n\n",
            idx + 1,
            title,
            published_date,
            truncate_text(snippet, 250),
            url
        ));
    }

    Ok(output)
}

/// Fallback: Generate search URLs when APIs unavailable
fn search_fallback(query: &str, _num_results: usize, source: &str) -> Result<String> {
    let mut output = String::new();

    output.push_str(&format!(
        "**Web Search: '{query}'**\n\n\
         To enable automatic search results, you can configure an Exa API key:\n\n\
         ```bash\n\
         export EXA_API_KEY=\"your-api-key-here\"\n```\n\n\
         Get your API key at: https://exa.ai (free tier available)\n\n\
         **Manual Search Links:**\n\n"
    ));

    match source {
        "news" => {
            output.push_str(&format!(
                "- [Google News]({})\n\
                 - [Bing News]({})\n\
                 - [DuckDuckGo News]({})\n",
                google_news_url(query),
                "https://www.bing.com/news",
                duckduckgo_news_url(query)
            ));
        }
        "web" => {
            output.push_str(&format!(
                "- [Wikipedia]({})\n\
                 - [Google Search]({})\n\
                 - [DuckDuckGo]({})\n",
                wikipedia_search_url(query),
                google_search_url(query),
                duckduckgo_search_url(query)
            ));
        }
        _ => {
            // Default to web search for unknown sources
        }
    }

    output.push_str("\n**Tip:** For factual questions, try adding 'wikipedia:' to your search to prioritize Wikipedia results.\n");

    Ok(output)
}

/// Generate Wikipedia search URL
fn wikipedia_search_url(query: &str) -> String {
    format!(
        "https://en.wikipedia.org/w/index.php?search={}",
        urlencoding::encode(query)
    )
}

/// Generate Google search URL
fn google_search_url(query: &str) -> String {
    format!(
        "https://www.google.com/search?q={}",
        urlencoding::encode(query)
    )
}

/// Generate `DuckDuckGo` search URL
fn duckduckgo_search_url(query: &str) -> String {
    format!("https://duckduckgo.com/?q={}", urlencoding::encode(query))
}

/// Generate Google News search URL
fn google_news_url(query: &str) -> String {
    format!(
        "https://news.google.com/search?q={}",
        urlencoding::encode(query)
    )
}

/// Generate `DuckDuckGo` News search URL
fn duckduckgo_news_url(query: &str) -> String {
    format!(
        "https://duckduckgo.com/?q=!news {}",
        urlencoding::encode(query)
    )
}

/// Truncate text to max length with ellipsis
fn truncate_text(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        return text.to_string();
    }

    // Truncate at a valid UTF-8 boundary first, then prefer a word boundary.
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
        assert_eq!(tool.name(), "web_search");
        assert!(tool.description().contains("current information"));
        assert_eq!(tool.permission(), ToolPermission::Network);
    }

    #[test]
    fn test_is_factual_query() {
        assert!(is_factual_query("Who is the Prime Minister of Thailand"));
        assert!(is_factual_query("current president of the United States"));
        assert!(is_factual_query("biography of Elon Musk"));
        assert!(!is_factual_query("how to parse JSON in Rust"));
        assert!(!is_factual_query("best restaurants in Bangkok"));
    }

    #[test]
    fn test_search_urls() {
        let pm_query = "Prime Minister of Thailand";
        let wiki_url = wikipedia_search_url(pm_query);
        assert!(wiki_url.contains("wikipedia.org"));
        assert!(wiki_url.contains("Prime"));

        let google_url = google_search_url(pm_query);
        assert!(google_url.contains("google.com"));
    }

    #[test]
    fn test_truncate_text() {
        let long_text = "This is a very long text that should be truncated at some point";
        let truncated = truncate_text(long_text, 30);
        assert!(truncated.len() <= 33); // 30 + "..."
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
