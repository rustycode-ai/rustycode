use crate::{Tool, ToolContext, ToolOutput, ToolPermission};
use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::env;

/// `CodeSearch` tool - Search for code examples and documentation
///
/// This tool searches for code examples, documentation, and implementation patterns.
/// It can use various search providers:
/// - Exa Code API (if API key is configured)
/// - Web search with structured queries
/// - Direct documentation links
pub struct CodeSearchTool;

impl Tool for CodeSearchTool {
    fn name(&self) -> &'static str {
        "codesearch"
    }

    fn description(&self) -> &'static str {
        r#"Search for code examples, documentation, and implementation patterns.

Use this tool when you need to:
- Find code examples for a specific library or framework
- Look up documentation for APIs, functions, or modules
- Find implementation patterns or best practices
- Search GitHub for code samples

The tool searches across:
- Official documentation sites
- GitHub repositories
- Stack Overflow
- Technical blogs and tutorials

**Examples:**
- "rust tokio async spawn example"
- "react useEffect cleanup function"
- "python requests post json"
- "typescript generic constraints"#
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Network
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["query"],
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query for code examples or documentation. Include programming language and specific terms."
                },
                "language": {
                    "type": "string",
                    "description": "Programming language to filter results (e.g., 'rust', 'python', 'javascript', 'typescript')",
                    "enum": ["rust", "python", "javascript", "typescript", "go", "java", "cpp", "c", "ruby", "php", "swift", "kotlin", "csharp", "shell", "markdown", "yaml", "json"]
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results to return (default: 5)",
                    "default": 5,
                    "minimum": 1,
                    "maximum": 20
                },
                "source": {
                    "type": "string",
                    "description": "Search source preference (default: 'auto')",
                    "enum": ["auto", "github", "docs", "stackoverflow", "web"],
                    "default": "auto"
                }
            }
        })
    }

    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let query = required_string(&params, "query")?;
        let language = optional_string(&params, "language");
        let max_results = params
            .get("max_results")
            .and_then(Value::as_u64)
            .unwrap_or(5) as usize;
        let source = optional_string(&params, "source").unwrap_or("auto");

        // Clamp max_results
        let max_results = max_results.clamp(1, 20);

        // Check for Exa API key
        let exa_api_key = env::var("EXA_API_KEY").ok();
        let has_api_key = exa_api_key.is_some();

        let results = if let Some(ref api_key) = exa_api_key {
            // Use Exa Code API
            search_with_exa(query, language, max_results, source, api_key)?
        } else {
            // Fallback: Generate search links and guidance
            generate_search_guidance(query, language, max_results, source)?
        };

        let output = format!("**Code Search Results for: `{query}`**\n\n{results}");

        // Build metadata
        let metadata = json!({
            "query": query,
            "language": language,
            "max_results": max_results,
            "source": source,
            "has_exa_api_key": has_api_key
        });

        Ok(ToolOutput::with_structured(output, metadata))
    }
}

/// Search using Exa Code API
fn search_with_exa(
    query: &str,
    language: Option<&str>,
    max_results: usize,
    _source: &str,
    api_key: &str,
) -> Result<String> {
    use reqwest::blocking::Client;

    // Build Exa API request
    let client = Client::new();
    let mut search_query = query.to_string();

    // Add language filter if specified
    if let Some(lang) = language {
        search_query = format!("{search_query} language:{lang}");
    }

    let request_body = json!({
        "query": search_query,
        "numResults": max_results,
        "useAutoprompt": true,
        "type": "keyword",
        "category": "code"
    });

    let response = client
        .post("https://api.exa.ai/search")
        .header("Content-Type", "application/json")
        .header("x-api-key", api_key)
        .json(&request_body)
        .send()
        .map_err(|e| anyhow!("Failed to call Exa API: {e}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .unwrap_or_else(|_| "Unable to read error".to_string());
        return Err(anyhow!(
            "Exa API request failed with status {status}: {error_text}"
        ));
    }

    let response_json: Value = response
        .json()
        .map_err(|e| anyhow!("Failed to parse Exa API response: {e}"))?;

    let results = response_json
        .get("results")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("Exa API response missing 'results' field"))?;

    let mut output = String::new();

    for (i, result) in results.iter().take(max_results).enumerate() {
        let title = result
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Untitled");
        let url = result.get("url").and_then(|v| v.as_str()).unwrap_or("#");
        let score = result.get("score").and_then(Value::as_f64).unwrap_or(0.0);

        output.push_str(&format!(
            "\n### {}. {} (relevance: {:.1}%)\n\n",
            i + 1,
            title,
            score * 100.0
        ));
        output.push_str(&format!("**URL:** {url}\n\n"));

        // Add excerpt if available
        if let Some(text) = result.get("text").and_then(|v| v.as_str()) {
            let excerpt = if text.len() > 500 {
                format!("{}...", text.chars().take(497).collect::<String>())
            } else {
                text.to_string()
            };
            output.push_str(&format!("**Excerpt:**\n```\n{excerpt}\n```\n"));
        }

        output.push_str("---\n");
    }

    Ok(output)
}

/// Generate search guidance and links when no API key is available
fn generate_search_guidance(
    query: &str,
    language: Option<&str>,
    _max_results: usize,
    source: &str,
) -> Result<String> {
    let mut output = String::new();

    output.push_str(
        "**Note:** To get actual search results, set up an Exa API key:\n\n\
         ```bash\n\
         export EXA_API_KEY=\"your-api-key-here\"\n```\n\n\
         Get your API key at: https://exa.ai\n\n\
         Below are curated search links for your query:\n\n",
    );

    // Generate search URLs based on source preference

    match source {
        "github" => {
            output.push_str(&format!(
                "### GitHub Code Search\n\
                 [Search GitHub]({})\n\n",
                github_search_url(query, language)
            ));
        }
        "docs" => {
            output.push_str(&format!(
                "### Documentation Search\n\
                 * [Google Docs Search]({})\n\
                 * [DevDocs]({})\n\n",
                google_search_url(&format!("{query} documentation")),
                devdocs_url(language)
            ));
        }
        "stackoverflow" => {
            output.push_str(&format!(
                "### Stack Overflow\n\
                 [Search Stack Overflow]({})\n\n",
                stackoverflow_search_url(query, language)
            ));
        }
        _ => {
            // Auto: provide all sources
            output.push_str("### Recommended Search Sources\n\n");

            // GitHub
            output.push_str(&format!(
                "**1. GitHub Code Search**\n\
                 [Search on GitHub]({})\n\
                 - Find actual code examples\n\
                 - See implementation patterns\n\
                 - Explore open-source projects\n\n",
                github_search_url(query, language)
            ));

            // Stack Overflow
            output.push_str(&format!(
                "**2. Stack Overflow**\n\
                 [Search Stack Overflow]({})\n\
                 - Find solutions to common problems\n\
                 - Learn from community discussions\n\
                 - See best practices\n\n",
                stackoverflow_search_url(query, language)
            ));

            // Documentation
            output.push_str(&format!(
                "**3. Official Documentation**\n\
                 * [Google Search - docs]({})\n\
                 * [DevDocs]({})\n\n",
                google_search_url(&format!("{query} documentation")),
                devdocs_url(language)
            ));
        }
    }

    // Add language-specific tips
    if let Some(lang) = language {
        output.push_str(&format!(
            "### Tips for searching in {lang}\n\
             * Use specific function/method names\n\
             * Include error messages for debugging\n\
             * Add 'example' or 'tutorial' for learning resources\n\
             * Try 'best practices' for implementation patterns\n\n"
        ));
    }

    Ok(output)
}

/// Generate `GitHub` search URL
fn github_search_url(query: &str, language: Option<&str>) -> String {
    let mut search_query = query.to_string();

    // Add language filter if specified
    if let Some(lang) = language {
        search_query = format!("{search_query} language:{lang}");
    }

    format!(
        "https://github.com/search?q={}&type=code",
        urlencoding::encode(&search_query)
    )
}

/// Generate Stack Overflow search URL
fn stackoverflow_search_url(query: &str, language: Option<&str>) -> String {
    let search_query = if let Some(lang) = language {
        format!("[{lang}] {query}")
    } else {
        query.to_string()
    };

    format!(
        "https://stackoverflow.com/search?q={}",
        urlencoding::encode(&search_query)
    )
}

/// Generate Google search URL
fn google_search_url(query: &str) -> String {
    format!(
        "https://www.google.com/search?q={}",
        urlencoding::encode(query)
    )
}

/// Generate `DevDocs` URL for language
fn devdocs_url(language: Option<&str>) -> String {
    match language {
        Some("rust") => "https://rust.docs.rs".to_string(),
        Some("python") => "https://docs.python.org/3/".to_string(),
        Some("javascript" | "typescript") => {
            "https://developer.mozilla.org/en-US/docs/Web/JavaScript".to_string()
        }
        Some("go") => "https://pkg.go.dev/".to_string(),
        Some("ruby") => "https://ruby-doc.org/".to_string(),
        Some("java") => "https://docs.oracle.com/en/java/javase/17/docs/api/".to_string(),
        _ => "https://devdocs.io/".to_string(),
    }
}

fn required_string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing string parameter `{key}`"))
}

fn optional_string<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_codesearch_tool_metadata() {
        let tool = CodeSearchTool;
        assert_eq!(tool.name(), "codesearch");
        assert!(tool.description().contains("code examples"));
        assert_eq!(tool.permission(), ToolPermission::Network);
    }

    #[test]
    fn test_codesearch_parameters_schema() {
        let tool = CodeSearchTool;
        let schema = tool.parameters_schema();

        assert_eq!(schema["type"], "object");
        let required = schema["required"].as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0], "query");

        // Check query property
        assert_eq!(schema["properties"]["query"]["type"], "string");

        // Check language enum values
        let languages = schema["properties"]["language"]["enum"].as_array().unwrap();
        assert!(languages.contains(&json!("rust")));
        assert!(languages.contains(&json!("python")));
        assert!(languages.contains(&json!("typescript")));

        // Check max_results constraints
        assert_eq!(schema["properties"]["max_results"]["default"], 5);
        assert_eq!(schema["properties"]["max_results"]["minimum"], 1);
        assert_eq!(schema["properties"]["max_results"]["maximum"], 20);

        // Check source enum
        let sources = schema["properties"]["source"]["enum"].as_array().unwrap();
        assert!(sources.contains(&json!("auto")));
        assert!(sources.contains(&json!("github")));
        assert!(sources.contains(&json!("docs")));
    }

    #[test]
    fn test_codesearch_missing_query() {
        let tool = CodeSearchTool;
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(json!({}), &ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("query"));
    }

    #[test]
    fn test_github_search_url() {
        let url = github_search_url("async spawn", Some("rust"));
        assert!(url.contains("github.com/search"));
        // urlencoding::encode uses %20 for spaces, not +
        assert!(url.contains("async%20spawn"));
        assert!(url.contains("language%3Arust"));
    }

    #[test]
    fn test_stackoverflow_search_url() {
        let url = stackoverflow_search_url("promise catch", Some("javascript"));
        assert!(url.contains("stackoverflow.com/search"));
        assert!(url.contains("javascript"));
        assert!(url.contains("promise"));
    }

    #[test]
    fn test_devdocs_url() {
        assert_eq!(devdocs_url(Some("rust")), "https://rust.docs.rs");
        assert_eq!(devdocs_url(Some("python")), "https://docs.python.org/3/");
        assert_eq!(
            devdocs_url(Some("javascript")),
            "https://developer.mozilla.org/en-US/docs/Web/JavaScript"
        );
        assert_eq!(devdocs_url(None), "https://devdocs.io/");
    }

    #[test]
    fn test_max_results_clamping() {
        let tool = CodeSearchTool;
        let ctx = ToolContext::new("/tmp");

        // Test with value below minimum
        let result = tool.execute(json!({ "query": "test", "max_results": 0 }), &ctx);
        assert!(result.is_ok());

        // Test with value above maximum
        let result = tool.execute(json!({ "query": "test", "max_results": 100 }), &ctx);
        assert!(result.is_ok());

        // Test with value in range
        let result = tool.execute(json!({ "query": "test", "max_results": 10 }), &ctx);
        assert!(result.is_ok());
    }

    #[test]
    fn test_search_guidance_generation() {
        let guidance = generate_search_guidance("async await", Some("rust"), 5, "auto").unwrap();

        assert!(guidance.contains("GitHub Code Search"));
        assert!(guidance.contains("Stack Overflow"));
        assert!(guidance.contains("Official Documentation"));
        assert!(guidance.contains("rust"));
    }

    #[test]
    fn test_source_specific_guidance() {
        // GitHub-only guidance
        let github_guidance =
            generate_search_guidance("http client", Some("rust"), 5, "github").unwrap();
        assert!(github_guidance.contains("GitHub Code Search"));
        assert!(!github_guidance.contains("Stack Overflow"));

        // Stack Overflow-only guidance
        let so_guidance =
            generate_search_guidance("error handling", Some("rust"), 5, "stackoverflow").unwrap();
        assert!(so_guidance.contains("Stack Overflow"));
        assert!(!so_guidance.contains("GitHub Code Search"));
    }
}
