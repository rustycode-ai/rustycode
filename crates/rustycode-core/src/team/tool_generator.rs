//! Natural Language Tool Generation.
//!
//! Generates tool wrappers from natural language descriptions and API specs.
//! Inspired by AutoAgent's Zero-Code Tool Generation.
//!
//! # Example
//!
//! ```ignore
//! let generator = ToolGenerator::new(llm_provider);
//!
//! let tool = generator.generate_from_description(
//!     "Fetch weather data from OpenWeatherMap API",
//!     &openweathermap_api_spec,
//! ).await?;
//!
//! // tool.code contains the generated Rust code
//! // tool.tool_def can be registered with the executor
//! ```

use anyhow::{Context, Result};
use rustycode_llm::{ChatMessage, CompletionRequest, CompletionResponse, LLMProvider};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, info};

/// Generated tool from natural language description.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedTool {
    /// Tool name (derived from description).
    pub name: String,
    /// Tool description for the LLM.
    pub description: String,
    /// Input schema (JSON Schema format).
    pub input_schema: serde_json::Value,
    /// Generated Rust code (if language generation enabled).
    pub code: Option<String>,
    /// Original description provided by user.
    pub original_description: String,
    /// Confidence score (0.0-1.0).
    pub confidence: f32,
}

/// API specification for tool generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiSpec {
    /// API name.
    pub name: String,
    pub base_url: String,
    /// Authentication type (none, api_key, bearer, oauth).
    pub auth_type: AuthType,
    /// API endpoints.
    pub endpoints: Vec<EndpointSpec>,
}

/// Authentication type.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthType {
    None,
    ApiKey,
    Bearer,
    OAuth,
}

/// Endpoint specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointSpec {
    /// Endpoint path.
    pub path: String,
    /// HTTP method.
    pub method: String,
    /// Description of what this endpoint does.
    pub description: String,
    /// Required parameters.
    pub required_params: Vec<ParamSpec>,
    /// Optional parameters.
    pub optional_params: Vec<ParamSpec>,
    /// Response schema description.
    pub response_description: String,
}

/// Parameter specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamSpec {
    /// Parameter name.
    pub name: String,
    /// Parameter type (string, integer, boolean, etc.).
    pub param_type: String,
    /// Parameter description.
    pub description: String,
    /// Whether this parameter is required.
    pub required: bool,
}

/// Tool Generator using LLM to create tools from descriptions.
pub struct ToolGenerator {
    /// LLM provider for code generation.
    llm: Arc<dyn LLMProvider>,
    /// Known API specs for reference.
    api_specs: Vec<ApiSpec>,
}

impl ToolGenerator {
    pub fn new(llm: Arc<dyn LLMProvider>) -> Self {
        Self {
            llm,
            api_specs: Vec::new(),
        }
    }

    /// Register an API spec for tool generation.
    pub fn register_api(&mut self, spec: ApiSpec) {
        self.api_specs.push(spec);
    }

    /// Generate a tool from a natural language description.
    ///
    pub async fn generate_from_description(
        &self,
        description: &str,
        context: Option<&ToolGenerationContext>,
    ) -> Result<GeneratedTool> {
        info!("Generating tool from description: {}", description);

        // Build the prompt for tool definition generation
        let prompt = self.build_generation_prompt(description, context);

        // Call LLM to generate tool definition
        let req = CompletionRequest::new(
            "",
            vec![ChatMessage::user(prompt.clone())],
        );
        let resp: CompletionResponse = self.llm.complete(req).await.map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let response = resp.content;
        if response.is_empty() {
            anyhow::bail!("LLM generation failed: empty response");
        }

        // Parse the response into a GeneratedTool
        let tool = self.parse_tool_response(&response, description)?;

        debug!(
            "Generated tool: {} (confidence: {:.2})",
            tool.name, tool.confidence
        );
        Ok(tool)
    }

    /// Generate a tool that wraps an existing API endpoint.
    pub async fn generate_from_api(
        &self,
        api_name: &str,
        endpoint_path: &str,
        description: &str,
    ) -> Result<GeneratedTool> {
        // Find the API spec
        let api_spec = self
            .api_specs
            .iter()
            .find(|api| api.name.eq_ignore_ascii_case(api_name))
            .with_context(|| format!("API '{}' not found", api_name))?;

        // Find the endpoint
        let endpoint = api_spec
            .endpoints
            .iter()
            .find(|ep| ep.path == endpoint_path)
            .with_context(|| {
                format!(
                    "Endpoint '{}' not found in API '{}'",
                    endpoint_path, api_name
                )
            })?;

        // Generate tool definition
        let tool = GeneratedTool {
            name: format!(
                "{}_{}",
                api_name.to_lowercase(),
                endpoint.path.replace('/', "_").trim_end_matches('_')
            ),
            description: description.to_string(),
            input_schema: self.build_input_schema(endpoint),
            code: Some(self.generate_api_wrapper_code(api_spec, endpoint)),
            original_description: description.to_string(),
            confidence: 0.9, // High confidence since we have a concrete API spec
        };

        Ok(tool)
    }

    /// Validate a generated tool for safety.
    pub fn validate_tool(&self, tool: &GeneratedTool) -> ValidationResult {
        let mut issues = Vec::new();

        // Check for dangerous patterns in generated code
        if let Some(ref code) = tool.code {
            if code.contains("std::process::Command") {
                issues.push(
                    "Generated code uses process execution - review for security".to_string(),
                );
            }
            if code.contains("unsafe") {
                issues.push("Generated code uses unsafe blocks - review carefully".to_string());
            }
            if code.contains("include_str!") || code.contains("include_bytes!") {
                issues.push("Generated code includes external files - verify paths".to_string());
            }
        }

        // Check input schema for overly permissive patterns
        if tool.input_schema.get("properties").is_none() {
            issues.push("Input schema missing properties - may be too permissive".to_string());
        }

        ValidationResult {
            is_valid: issues.is_empty(),
            issues,
            tool_name: tool.name.clone(),
        }
    }

    /// Build the generation prompt for the LLM.
    fn build_generation_prompt(
        &self,
        description: &str,
        context: Option<&ToolGenerationContext>,
    ) -> String {
        let mut prompt = String::from(
            r#"You are a tool definition generator for rustycode.

Given a natural language description, generate a tool definition with:
1. A concise name (snake_case)
2. A clear description for the LLM
3. Input schema in JSON Schema format
4. (Optional) Rust code implementation

## Example

Description: "Fetch weather data from OpenWeatherMap API for a given city"

Response:
{
  "name": "fetch_weather",
  "description": "Fetch current weather data for a city using OpenWeatherMap API",
  "input_schema": {
    "type": "object",
    "properties": {
      "city": {
        "type": "string",
        "description": "City name (e.g., 'London')"
      },
      "units": {
        "type": "string",
        "enum": ["metric", "imperial", "standard"],
        "default": "metric",
        "description": "Temperature units"
      }
    },
    "required": ["city"]
  }
}

## Available APIs

"#,
        );

        // Include available API specs as reference
        for api in &self.api_specs {
            prompt.push_str(&format!("\n### API: {}\n", api.name));
            prompt.push_str(&format!("Base URL: {}\n", api.base_url));
            prompt.push_str(&format!("Auth: {:?}\n\n", api.auth_type));

            for endpoint in &api.endpoints {
                prompt.push_str(&format!(
                    "- **{} {}**: {}\n",
                    endpoint.method, endpoint.path, endpoint.description
                ));
            }
        }

        // Include context if provided
        if let Some(ctx) = context {
            prompt.push_str("\n## Existing Tools\n\n");
            for tool in &ctx.existing_tools {
                prompt.push_str(&format!("- {}\n", tool));
            }

            prompt.push_str("\n## Patterns to Follow\n\n");
            for pattern in &ctx.patterns {
                prompt.push_str(&format!("- {}\n", pattern));
            }
        }

        prompt.push_str("\n## Task\n\n");
        prompt.push_str(&format!("Generate a tool for: {}\n", description));

        prompt
    }

    /// Parse the LLM response into a GeneratedTool.
    fn parse_tool_response(
        &self,
        response: &str,
        original_description: &str,
    ) -> Result<GeneratedTool> {
        // Try to extract JSON from the response
        let json_str =
            extract_json_from_response(response).context("Could not find JSON in response")?;

        // Parse the JSON into a GeneratedTool
        let parsed: serde_json::Value =
            serde_json::from_str(json_str).context("Invalid JSON in response")?;

        let name = parsed
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("generated_tool")
            .to_string();

        let description = parsed
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("Generated tool")
            .to_string();

        let input_schema = parsed.get("input_schema").cloned().unwrap_or_else(|| {
            serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            })
        });

        let code = parsed
            .get("code")
            .and_then(|v| v.as_str())
            .map(String::from);

        let confidence = parsed
            .get("confidence")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.7) as f32;

        Ok(GeneratedTool {
            name,
            description,
            input_schema,
            code,
            original_description: original_description.to_string(),
            confidence,
        })
    }

    /// Build input schema from an endpoint spec.
    fn build_input_schema(&self, endpoint: &EndpointSpec) -> serde_json::Value {
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        // Add required parameters
        for param in &endpoint.required_params {
            properties.insert(
                param.name.clone(),
                serde_json::json!({
                    "type": param.param_type,
                    "description": param.description
                }),
            );
            required.push(param.name.clone());
        }

        // Add optional parameters
        for param in &endpoint.optional_params {
            let schema = serde_json::json!({
                "type": param.param_type,
                "description": param.description
            });
            // Note: We don't add "default" here since we don't know sensible defaults
            properties.insert(param.name.clone(), schema);
        }

        serde_json::json!({
            "type": "object",
            "properties": properties,
            "required": required
        })
    }

    /// Generate Rust wrapper code for an API endpoint.
    fn generate_api_wrapper_code(&self, api: &ApiSpec, endpoint: &EndpointSpec) -> String {
        let fn_name = format!(
            "{}_{}",
            api.name.to_lowercase(),
            endpoint.path.replace('/', "_").trim_end_matches('_')
        );
        let method_fn = match endpoint.method.to_uppercase().as_str() {
            "GET" => "get",
            "POST" => "post",
            "PUT" => "put",
            "DELETE" => "delete",
            "PATCH" => "patch",
            _ => "get",
        };

        let mut code = format!(
            r#"/// {description}
///
/// API: {api_name} {method} {path}
pub async fn {fn_name}(
    client: &reqwest::Client,
    api_key: &str,
"#,
            description = endpoint.description,
            api_name = api.name,
            method = endpoint.method,
            path = endpoint.path,
            fn_name = fn_name,
        );

        // Add parameters as function arguments
        for param in &endpoint.required_params {
            let rust_type = self.param_to_rust_type(&param.param_type);
            code.push_str(&format!("    {}: {},\n", param.name, rust_type));
        }

        code.push_str(&format!(
            r#") -> anyhow::Result<{api_name}Response> {{
    let url = format!("{{}}{{}}", {api_name}_BASE_URL, "{path}");
"#,
            api_name = api.name.to_uppercase(),
            path = endpoint.path,
        ));

        // Add query params or body
        if !endpoint.required_params.is_empty() && endpoint.method == "GET" {
            code.push_str("    let mut query_params = Vec::new();\n");
            for param in &endpoint.required_params {
                code.push_str(&format!(
                    "    query_params.push((\"{}\", {}.to_string()));\n",
                    param.name, param.name
                ));
            }
            code.push_str("    let client = client.query(&query_params);\n");
        }

        // Add auth header
        match api.auth_type {
            AuthType::ApiKey => {
                code.push_str("    let client = client.header(\"X-API-Key\", api_key);\n");
            }
            AuthType::Bearer => {
                code.push_str("    let client = client.header(\"Authorization\", format!(\"Bearer {}\", api_key));\n");
            }
            _ => {}
        }

        code.push_str(&format!(
            r#"
    let response = client.{method}(&url).send().await?;
    let result: {api_name}Response = response.json().await?;
    Ok(result)
}}"#,
            method = method_fn,
            api_name = api.name.to_uppercase(),
        ));

        code
    }

    /// Convert a parameter type string to Rust type.
    fn param_to_rust_type(&self, param_type: &str) -> &str {
        match param_type {
            "string" => "&str",
            "integer" => "i64",
            "number" => "f64",
            "boolean" => "bool",
            "array" => "&[serde_json::Value]",
            _ => "&str",
        }
    }
}

/// Context for tool generation.
#[derive(Debug, Clone, Default)]
pub struct ToolGenerationContext {
    /// Existing tools to avoid duplicates.
    pub existing_tools: Vec<String>,
    /// Coding patterns to follow.
    pub patterns: Vec<String>,
}

/// Result of tool validation.
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Whether the tool passed validation.
    pub is_valid: bool,
    /// List of issues found.
    pub issues: Vec<String>,
    /// Name of the tool that was validated.
    pub tool_name: String,
}

impl ValidationResult {
    /// Check if validation passed with no issues.
    pub fn passed(&self) -> bool {
        self.is_valid
    }

    /// Get a summary of issues for display.
    pub fn summary(&self) -> String {
        if self.issues.is_empty() {
            format!("Tool '{}' passed validation", self.tool_name)
        } else {
            format!(
                "Tool '{}' has {} issues:\n{}",
                self.tool_name,
                self.issues.len(),
                self.issues
                    .iter()
                    .map(|s| format!("  - {}", s))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        }
    }
}

/// Extract JSON from an LLM response (handles markdown-wrapped JSON).
fn extract_json_from_response(response: &str) -> Option<&str> {
    // Try to find JSON wrapped in markdown code fences
    if let Some(start) = response.find("```json") {
        let start = start + 7;
        if let Some(end) = response[start..].find("```") {
            return Some(&response[start..start + end]);
        }
    }

    // Try to find JSON object directly
    if let Some(start) = response.find('{') {
        let mut depth = 0;
        let mut in_string = false;
        let mut escape_next = false;

        for (i, c) in response[start..].char_indices() {
            if escape_next {
                escape_next = false;
                continue;
            }

            match c {
                '"' if !escape_next => in_string = !in_string,
                '\\' if in_string => escape_next = true,
                '{' if !in_string => depth += 1,
                '}' if !in_string => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(&response[start..start + i + 1]);
                    }
                }
                _ => {}
            }
        }
    }

    None
}


#[cfg(test)]
struct MockLLM {
    response: String,
}

#[cfg(test)]
impl MockLLM {
    fn with_response(response: &str) -> Self {
        Self {
            response: response.to_string(),
        }
    }
}

#[cfg(test)]
#[async_trait::async_trait]
impl LLMProvider for MockLLM {
    fn name(&self) -> &'static str {
        "mock"
    }

    fn supports_streaming(&self) -> bool {
        false
    }

    async fn is_available(&self) -> bool {
        true
    }

    async fn list_models(&self) -> Result<Vec<String>, rustycode_llm::ProviderError> {
        Ok(vec![])
    }

    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> Result<CompletionResponse, rustycode_llm::ProviderError> {
        Ok(CompletionResponse {
            content: self.response.clone(),
            model: "mock".to_string(),
            usage: None,
            stop_reason: Some("end_turn".to_string()),
            citations: None,
            thinking_blocks: None,
            structured_output: None,
        })
    }

    async fn complete_stream(
        &self,
        _request: CompletionRequest,
    ) -> Result<std::pin::Pin<Box<dyn futures::Stream<Item = rustycode_llm::StreamChunk> + Send>>, rustycode_llm::ProviderError> {
        Err(rustycode_llm::ProviderError::Api("mock does not support streaming".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json_from_markdown() {
        let response = r#"Here's the tool:
```json
{
  "name": "test_tool",
  "description": "A test tool"
}
```
"#;

        let json = extract_json_from_response(response).unwrap();
        assert!(json.contains("\"name\": \"test_tool\""));
    }

    #[test]
    fn test_extract_json_plain() {
        let response = r#"{"name": "test_tool", "description": "A test tool"}"#;

        let json = extract_json_from_response(response).unwrap();
        assert_eq!(
            json,
            r#"{"name": "test_tool", "description": "A test tool"}"#
        );
    }

    #[test]
    fn test_validation_result() {
        let result = ValidationResult {
            is_valid: false,
            issues: vec!["Uses unsafe".to_string()],
            tool_name: "test_tool".to_string(),
        };

        assert!(!result.passed());
        assert!(result.summary().contains("unsafe"));
    }

    #[test]
    fn test_param_to_rust_type() {
        let generator = ToolGenerator::new(Arc::new(MockLLM::with_response("{}")));

        assert_eq!(generator.param_to_rust_type("string"), "&str");
        assert_eq!(generator.param_to_rust_type("integer"), "i64");
        assert_eq!(generator.param_to_rust_type("boolean"), "bool");
    }

    #[tokio::test]
    async fn test_generate_from_api() {
        let mut generator = ToolGenerator::new(Arc::new(MockLLM::with_response("{}")));

        generator.register_api(ApiSpec {
            name: "Weather".to_string(),
            base_url: "https://api.weather.com".to_string(),
            auth_type: AuthType::ApiKey,
            endpoints: vec![EndpointSpec {
                path: "/current".to_string(),
                method: "GET".to_string(),
                description: "Get current weather".to_string(),
                required_params: vec![ParamSpec {
                    name: "city".to_string(),
                    param_type: "string".to_string(),
                    description: "City name".to_string(),
                    required: true,
                }],
                optional_params: vec![],
                response_description: "Weather data".to_string(),
            }],
        });

        let tool = generator
            .generate_from_api("Weather", "/current", "Get weather for a city")
            .await
            .unwrap();

        assert!(tool.name.contains("weather"));
        assert!(tool.code.is_some());
        assert_eq!(tool.confidence, 0.9);
    }
}
