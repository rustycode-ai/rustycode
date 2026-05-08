//! Tool execution types for RustyCode
//!
//! Tool calls are the primary mechanism for interacting with the system,
//! allowing the LLM to read files, execute commands, and perform other operations.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A request to invoke a named tool.
///
/// Tool calls are the primary mechanism for interacting with the system,
/// allowing the LLM to read files, execute commands, and perform other operations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolCall {
    /// Stable, opaque identifier correlating this call to its result
    pub call_id: String,
    /// Name of the tool to invoke (e.g., "read_file", "bash")
    pub name: String,
    /// JSON object whose shape is defined by the tool's parameters_schema
    pub arguments: serde_json::Value,
}

impl ToolCall {
    pub fn new(
        call_id: impl Into<String>,
        name: impl Into<String>,
        arguments: serde_json::Value,
    ) -> Self {
        Self {
            call_id: call_id.into(),
            name: name.into(),
            arguments,
        }
    }

    /// Create a tool call with a generated ID
    pub fn with_generated_id(name: impl Into<String>, arguments: serde_json::Value) -> Self {
        Self {
            call_id: generate_call_id(),
            name: name.into(),
            arguments,
        }
    }
}

/// The outcome of a tool invocation.
///
/// Contains result of executing a tool call, including output, errors,
/// and optional structured data.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolResult {
    /// Matches the call_id from the originating ToolCall
    pub call_id: String,
    /// The output produced by the tool (stdout, file contents, etc.)
    pub output: String,
    /// Error if the tool failed
    pub error: Option<String>,
    /// Optional exit code from process execution
    pub exit_code: Option<i32>,
    /// Whether the tool execution was successful
    pub success: bool,
    /// Optional structured data (e.g., for tool with rich output)
    pub data: Option<serde_json::Value>,
    /// If set, the tool requests a CWD change to this path
    #[serde(skip)]
    pub new_cwd: Option<PathBuf>,
}

impl ToolResult {
    /// Create a successful tool result
    pub fn success(call_id: impl Into<String>, output: impl Into<String>) -> Self {
        Self {
            call_id: call_id.into(),
            output: output.into(),
            error: None,
            exit_code: Some(0),
            success: true,
            data: None,
            new_cwd: None,
        }
    }

    /// Create a failed tool result
    pub fn error(call_id: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            call_id: call_id.into(),
            output: String::new(),
            error: Some(error.into()),
            exit_code: None,
            success: false,
            data: None,
            new_cwd: None,
        }
    }

    /// Create a failed tool result with exit code
    pub fn error_with_code(
        call_id: impl Into<String>,
        error: impl Into<String>,
        exit_code: i32,
    ) -> Self {
        Self {
            call_id: call_id.into(),
            output: String::new(),
            error: Some(error.into()),
            exit_code: Some(exit_code),
            success: false,
            data: None,
            new_cwd: None,
        }
    }

    /// Check if the result indicates success
    pub fn is_success(&self) -> bool {
        self.success && self.error.is_none()
    }

    /// Check if the result indicates an error
    pub fn is_error(&self) -> bool {
        self.error.is_some()
    }
}

/// Permission level required for tool execution.
///
/// Tools can require different permission levels based on their potential impact.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ToolPermission {
    /// Tool can read data
    Read,
    /// Tool can write/modify data
    Write,
    /// Tool can execute commands
    Execute,
    /// Tool can run without asking
    AutoAllow,
    /// Tool requires user confirmation
    RequiresConfirmation,
    /// Tool is blocked (e.g., in planning mode)
    Blocked,
}

/// Metadata about a tool.
///
/// Provides information about a tool's capabilities, requirements, and permissions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolMetadata {
    /// Name of the tool
    pub name: String,
    /// Human-readable description
    pub description: String,
    /// JSON Schema for the tool's parameters
    pub parameters_schema: serde_json::Value,
    /// Permission level required
    pub permission: ToolPermission,
    /// Whether the tool modifies state
    pub mutates: bool,
    /// Tags for categorizing the tool
    pub tags: Vec<String>,
}

impl ToolMetadata {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters_schema: serde_json::Value,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters_schema,
            permission: ToolPermission::RequiresConfirmation,
            mutates: false,
            tags: Vec::new(),
        }
    }

    /// Set the permission level
    pub fn with_permission(mut self, permission: ToolPermission) -> Self {
        self.permission = permission;
        self
    }

    /// Set whether the tool mutates state
    pub fn with_mutates(mut self, mutates: bool) -> Self {
        self.mutates = mutates;
        self
    }

    /// Add a tag to the tool
    pub fn add_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }
}

/// Record of an API/tool call for cost tracking and auditing
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApiCall {
    pub model: String,
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub cost_usd: f64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub tool_name: Option<String>,
    /// Tokens served from cache (billed at 0.1× base input price)
    #[serde(default)]
    pub cache_read_tokens: u32,
    /// Tokens written to cache (billed at 1.25× base input price)
    #[serde(default)]
    pub cache_creation_tokens: u32,
    /// Estimated cost savings from cache hits (0.9 × base price × cache_read_tokens)
    #[serde(default)]
    pub cache_savings_usd: f64,
}

/// Provider interface for recording API call costs
pub trait CostTrackerProvider: Send + Sync {
    fn record_call(&self, call: ApiCall) -> anyhow::Result<()>;
}

/// Generate a unique call ID
fn generate_call_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("call_{:x}{:x}", nanos, counter)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_call_creation() {
        let call = ToolCall::new(
            "test-call",
            "read_file",
            serde_json::json!({"path": "/tmp/test.txt"}),
        );

        assert_eq!(call.call_id, "test-call");
        assert_eq!(call.name, "read_file");
    }

    #[test]
    fn test_tool_result_success() {
        let result = ToolResult::success("call-1", "File contents");

        assert!(result.is_success());
        assert!(!result.is_error());
        assert_eq!(result.output, "File contents");
        assert!(result.error.is_none());
        assert_eq!(result.exit_code, Some(0));
    }

    #[test]
    fn test_tool_result_error() {
        let result = ToolResult::error_with_code("call-1", "File not found", 1);

        assert!(!result.is_success());
        assert!(result.is_error());
        assert_eq!(result.error.as_ref().unwrap(), "File not found");
        assert_eq!(result.exit_code, Some(1));
    }

    #[test]
    fn test_tool_metadata_builder() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"}
            }
        });

        let metadata = ToolMetadata::new("read_file", "Read a file", schema)
            .with_permission(ToolPermission::AutoAllow)
            .with_mutates(false)
            .add_tag("fs")
            .add_tag("read");

        assert_eq!(metadata.name, "read_file");
        assert_eq!(metadata.permission, ToolPermission::AutoAllow);
        assert!(!metadata.mutates);
        assert_eq!(metadata.tags.len(), 2);
    }

    #[test]
    fn test_generate_call_id() {
        let id1 = generate_call_id();
        let id2 = generate_call_id();

        assert!(id1.starts_with("call_"));
        assert!(id2.starts_with("call_"));
        assert_ne!(id1, id2); // Should be unique
    }

    #[test]
    fn test_generate_call_id_uniqueness_across_many() {
        let ids: Vec<String> = (0..100).map(|_| generate_call_id()).collect();
        let unique: std::collections::HashSet<&str> = ids.iter().map(|s| s.as_str()).collect();
        assert_eq!(unique.len(), 100, "All 100 IDs should be unique");
    }

    #[test]
    fn test_generate_call_id_format() {
        let id = generate_call_id();
        // Format is call_{hex_timestamp}{hex_counter}
        let parts: Vec<&str> = id.splitn(2, '_').collect();
        assert_eq!(parts[0], "call");
        // The hex part should be non-empty
        assert!(!parts[1].is_empty());
        // Should only contain hex chars
        assert!(parts[1].chars().all(|c| c.is_ascii_hexdigit()));
    }

    // --- ToolPermission serde ---

    #[test]
    fn tool_permission_serde_variants() {
        let variants = vec![
            ToolPermission::Read,
            ToolPermission::Write,
            ToolPermission::Execute,
            ToolPermission::AutoAllow,
            ToolPermission::RequiresConfirmation,
            ToolPermission::Blocked,
        ];
        for v in &variants {
            let json = serde_json::to_string(v).unwrap();
            let decoded: ToolPermission = serde_json::from_str(&json).unwrap();
            assert_eq!(*v, decoded);
        }
    }

    #[test]
    fn tool_permission_renames() {
        assert_eq!(
            serde_json::to_string(&ToolPermission::AutoAllow).unwrap(),
            "\"auto_allow\""
        );
        assert_eq!(
            serde_json::to_string(&ToolPermission::Read).unwrap(),
            "\"read\""
        );
        assert_eq!(
            serde_json::to_string(&ToolPermission::RequiresConfirmation).unwrap(),
            "\"requires_confirmation\""
        );
    }

    // --- ToolCall serde ---

    #[test]
    fn tool_call_serde_roundtrip() {
        let call = ToolCall::new("c1", "bash", serde_json::json!({"cmd": "ls"}));
        let json = serde_json::to_string(&call).unwrap();
        let decoded: ToolCall = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.call_id, "c1");
        assert_eq!(decoded.name, "bash");
        assert_eq!(decoded.arguments["cmd"], "ls");
    }

    #[test]
    fn tool_call_with_generated_id_has_unique_ids() {
        let a = ToolCall::with_generated_id("read", serde_json::json!({}));
        let b = ToolCall::with_generated_id("read", serde_json::json!({}));
        assert_ne!(a.call_id, b.call_id);
        assert!(a.call_id.starts_with("call_"));
    }

    // --- ToolResult serde and edge cases ---

    #[test]
    fn tool_result_serde_roundtrip() {
        let result = ToolResult::success("c2", "hello");
        let json = serde_json::to_string(&result).unwrap();
        let decoded: ToolResult = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.call_id, "c2");
        assert_eq!(decoded.output, "hello");
        assert!(decoded.success);
        assert_eq!(decoded.exit_code, Some(0));
        assert!(decoded.error.is_none());
    }

    #[test]
    fn tool_result_error_serde_roundtrip() {
        let result = ToolResult::error_with_code("c3", "file not found", 1);
        let json = serde_json::to_string(&result).unwrap();
        let decoded: ToolResult = serde_json::from_str(&json).unwrap();
        assert!(!decoded.success);
        assert_eq!(decoded.error, Some("file not found".to_string()));
        assert_eq!(decoded.exit_code, Some(1));
        assert!(decoded.output.is_empty());
    }

    #[test]
    fn tool_result_with_data() {
        let mut result = ToolResult::success("c4", "ok");
        result.data = Some(serde_json::json!({"files": 3}));
        assert!(result.is_success());
        assert_eq!(result.data.unwrap()["files"], 3);
    }

    #[test]
    fn tool_result_is_success_requires_no_error() {
        // success=true but with error string -> is_success is false
        let result = ToolResult {
            call_id: "c5".into(),
            output: "partial".into(),
            error: Some("warning".into()),
            exit_code: Some(0),
            success: true,
            data: None,
            new_cwd: None,
        };
        assert!(!result.is_success()); // error is Some
    }

    // --- ToolMetadata serde ---

    #[test]
    fn tool_metadata_serde_roundtrip() {
        let meta = ToolMetadata::new(
            "bash",
            "Run commands",
            serde_json::json!({"type": "object"}),
        )
        .with_permission(ToolPermission::Execute)
        .with_mutates(true)
        .add_tag("shell")
        .add_tag("dangerous");
        let json = serde_json::to_string(&meta).unwrap();
        let decoded: ToolMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.name, "bash");
        assert_eq!(decoded.permission, ToolPermission::Execute);
        assert!(decoded.mutates);
        assert_eq!(decoded.tags, vec!["shell", "dangerous"]);
    }
}
