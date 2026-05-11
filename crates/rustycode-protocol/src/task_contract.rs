//! Typed task contracts for multi-agent communication.
//!
//! Every task that crosses an agent boundary has a contract: typed input,
//! typed output, JSON schemas, and validation. Contracts are registered
//! in a central [`TaskRegistry`] and enforced at dispatch and completion
//! time so mismatches are caught early rather than silently propagated.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

/// Error produced when a contract is violated.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContractViolation {
    /// Machine-readable code.
    pub code: ViolationCode,
    /// Human-readable message.
    pub message: String,
    /// Which field or value failed (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
}

/// Machine-readable violation codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViolationCode {
    /// Input failed schema or semantic validation.
    InvalidInput,
    /// Output failed schema or semantic validation.
    InvalidOutput,
    /// No contract registered for the given task name.
    UnknownTask,
    /// Task execution exceeded its deadline.
    Timeout,
    /// Task execution was retried too many times.
    RetriesExhausted,
}

impl fmt::Display for ContractViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let code = match self.code {
            ViolationCode::InvalidInput => "INVALID_INPUT",
            ViolationCode::InvalidOutput => "INVALID_OUTPUT",
            ViolationCode::UnknownTask => "UNKNOWN_TASK",
            ViolationCode::Timeout => "TIMEOUT",
            ViolationCode::RetriesExhausted => "RETRIES_EXHAUSTED",
        };
        match &self.field {
            Some(field) => write!(f, "[{}] {} (field: {})", code, self.message, field),
            None => write!(f, "[{}] {}", code, self.message),
        }
    }
}

impl std::error::Error for ContractViolation {}

impl ContractViolation {
    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self {
            code: ViolationCode::InvalidInput,
            message: message.into(),
            field: None,
        }
    }

    pub fn invalid_input_field(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: ViolationCode::InvalidInput,
            message: message.into(),
            field: Some(field.into()),
        }
    }

    pub fn invalid_output(message: impl Into<String>) -> Self {
        Self {
            code: ViolationCode::InvalidOutput,
            message: message.into(),
            field: None,
        }
    }

    pub fn unknown_task(name: impl Into<String>) -> Self {
        Self {
            code: ViolationCode::UnknownTask,
            message: format!("no contract registered for task '{}'", name.into()),
            field: None,
        }
    }

    pub fn timeout(message: impl Into<String>) -> Self {
        Self {
            code: ViolationCode::Timeout,
            message: message.into(),
            field: None,
        }
    }

    pub fn retries_exhausted(message: impl Into<String>) -> Self {
        Self {
            code: ViolationCode::RetriesExhausted,
            message: message.into(),
            field: None,
        }
    }
}

/// Retry policy for a task contract.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Maximum number of retries (0 = no retries).
    pub max_retries: u32,
    /// Backoff strategy.
    pub strategy: RetryStrategy,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 0,
            strategy: RetryStrategy::Fixed { interval_ms: 1000 },
        }
    }
}

/// Backoff strategy for retries.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RetryStrategy {
    /// Fixed interval between retries.
    Fixed { interval_ms: u64 },
    /// Exponential backoff: interval * 2^attempt.
    Exponential { base_ms: u64, max_ms: u64 },
}

impl RetryPolicy {
    /// No retries.
    pub const NONE: RetryPolicy = RetryPolicy {
        max_retries: 0,
        strategy: RetryStrategy::Fixed { interval_ms: 0 },
    };

    /// Compute the wait duration for a given attempt (0-indexed).
    pub fn wait_duration(&self, attempt: u32) -> Duration {
        match &self.strategy {
            RetryStrategy::Fixed { interval_ms } => Duration::from_millis(*interval_ms),
            RetryStrategy::Exponential { base_ms, max_ms } => {
                let exp = u64::from(attempt).min(10);
                let raw = u64::from(*base_ms).saturating_mul(1u64 << exp);
                Duration::from_millis(raw.min(*max_ms))
            }
        }
    }
}

/// Static metadata describing a task contract.
///
/// Each contract declares a unique name, JSON schemas for input and output,
/// an optional timeout, and a retry policy. The registry stores these
/// descriptors so dispatchers can look them up at runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDescriptor {
    /// Unique task name (e.g., "file.read", "bash.execute").
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// JSON Schema for the task input.
    pub input_schema: serde_json::Value,
    /// JSON Schema for the task output.
    pub output_schema: serde_json::Value,
    /// Maximum execution time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<Duration>,
    /// Retry policy (default: no retries).
    #[serde(default)]
    pub retry_policy: RetryPolicy,
    /// Tags for categorisation.
    #[serde(default)]
    pub tags: Vec<String>,
}

impl TaskDescriptor {
    /// Create a new descriptor with the given name, description, and schemas.
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: serde_json::Value,
        output_schema: serde_json::Value,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema,
            output_schema,
            timeout: None,
            retry_policy: RetryPolicy::default(),
            tags: Vec::new(),
        }
    }

    /// Set the timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Set the retry policy.
    pub fn with_retry_policy(mut self, policy: RetryPolicy) -> Self {
        self.retry_policy = policy;
        self
    }

    /// Add a tag.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }
}

/// Validation function type.
///
/// Receives a `serde_json::Value` and returns `Ok(())` if valid, or
/// a [`ContractViolation`] describing the problem.
pub type ValidatorFn =
    Arc<dyn Fn(&serde_json::Value) -> Result<(), ContractViolation> + Send + Sync>;

/// A fully-resolved contract: descriptor + optional validators.
pub struct TaskContract {
    pub descriptor: TaskDescriptor,
    /// Optional semantic validator for input (runs after schema check).
    pub input_validator: Option<ValidatorFn>,
    /// Optional semantic validator for output (runs after schema check).
    pub output_validator: Option<ValidatorFn>,
}

impl TaskContract {
    /// Create a contract from a descriptor with no custom validators.
    pub fn new(descriptor: TaskDescriptor) -> Self {
        Self {
            descriptor,
            input_validator: None,
            output_validator: None,
        }
    }

    /// Create a contract with a custom input validator.
    pub fn with_input_validator(
        descriptor: TaskDescriptor,
        validator: impl Fn(&serde_json::Value) -> Result<(), ContractViolation> + Send + Sync + 'static,
    ) -> Self {
        Self {
            descriptor,
            input_validator: Some(Arc::new(validator)),
            output_validator: None,
        }
    }

    /// Create a contract with custom input and output validators.
    pub fn with_validators(
        descriptor: TaskDescriptor,
        input: impl Fn(&serde_json::Value) -> Result<(), ContractViolation> + Send + Sync + 'static,
        output: impl Fn(&serde_json::Value) -> Result<(), ContractViolation> + Send + Sync + 'static,
    ) -> Self {
        Self {
            descriptor,
            input_validator: Some(Arc::new(input)),
            output_validator: Some(Arc::new(output)),
        }
    }

    /// Validate input against this contract.
    ///
    /// Runs schema structural check, then the custom semantic validator
    /// (if any).
    pub fn validate_input(&self, input: &serde_json::Value) -> Result<(), ContractViolation> {
        self.validate_schema(input, &self.descriptor.input_schema, true)?;
        if let Some(validator) = &self.input_validator {
            validator(input)?;
        }
        Ok(())
    }

    /// Validate output against this contract.
    pub fn validate_output(&self, output: &serde_json::Value) -> Result<(), ContractViolation> {
        self.validate_schema(output, &self.descriptor.output_schema, false)?;
        if let Some(validator) = &self.output_validator {
            validator(output)?;
        }
        Ok(())
    }

    /// Basic structural schema check.
    ///
    /// Validates that required top-level properties exist and have the
    /// expected types. This is intentionally lightweight — full JSON
    /// Schema validation would require a heavy dependency.
    fn validate_schema(
        &self,
        value: &serde_json::Value,
        schema: &serde_json::Value,
        is_input: bool,
    ) -> Result<(), ContractViolation> {
        let obj = match value {
            serde_json::Value::Object(m) => m,
            _other => {
                // If schema says "any" or is not an object schema, skip.
                if schema.get("type").and_then(|t| t.as_str()) == Some("object") {
                    let code = if is_input {
                        "input must be a JSON object"
                    } else {
                        "output must be a JSON object"
                    };
                    return Err(ContractViolation {
                        code: if is_input {
                            ViolationCode::InvalidInput
                        } else {
                            ViolationCode::InvalidOutput
                        },
                        message: code.to_string(),
                        field: None,
                    });
                }
                return Ok(());
            }
        };

        // Check required properties.
        if let Some(required) = schema.get("required").and_then(|r| r.as_array()) {
            for req in required {
                if let Some(name) = req.as_str() {
                    if !obj.contains_key(name) {
                        let code = if is_input {
                            ViolationCode::InvalidInput
                        } else {
                            ViolationCode::InvalidOutput
                        };
                        return Err(ContractViolation {
                            code,
                            message: format!("missing required property '{}'", name),
                            field: Some(name.to_string()),
                        });
                    }
                }
            }
        }

        // Check property types.
        if let Some(properties) = schema.get("properties").and_then(|p| p.as_object()) {
            for (name, prop_schema) in properties {
                if let Some(actual) = obj.get(name) {
                    if let Some(expected_type) = prop_schema.get("type").and_then(|t| t.as_str()) {
                        let type_ok = match expected_type {
                            "string" => actual.is_string(),
                            "number" | "integer" => actual.is_number(),
                            "boolean" => actual.is_boolean(),
                            "array" => actual.is_array(),
                            "object" => actual.is_object(),
                            _ => true,
                        };
                        if !type_ok {
                            let code = if is_input {
                                ViolationCode::InvalidInput
                            } else {
                                ViolationCode::InvalidOutput
                            };
                            return Err(ContractViolation {
                                code,
                                message: format!(
                                    "property '{}' expected type '{}', got '{}'",
                                    name,
                                    expected_type,
                                    json_type_name(actual)
                                ),
                                field: Some(name.clone()),
                            });
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

/// Human-readable JSON value type name.
fn json_type_name(v: &serde_json::Value) -> &'static str {
    match v {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

/// Central registry of task contracts.
///
/// Agents register their contracts at startup. Dispatchers look up contracts
/// before sending or receiving messages, enforcing the schema at both ends.
#[derive(Debug, Clone, Default)]
pub struct TaskRegistry {
    contracts: HashMap<String, TaskDescriptor>,
}

impl TaskRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a contract. Returns `Ok(())` on success, or an error if
    /// a contract with the same name already exists.
    pub fn register(&mut self, descriptor: TaskDescriptor) -> Result<(), ContractViolation> {
        if self.contracts.contains_key(&descriptor.name) {
            return Err(ContractViolation {
                code: ViolationCode::InvalidInput,
                message: format!("a contract for '{}' is already registered", descriptor.name),
                field: None,
            });
        }
        let name = descriptor.name.clone();
        self.contracts.insert(name, descriptor);
        Ok(())
    }

    /// Look up a contract by name.
    pub fn get(&self, name: &str) -> Option<&TaskDescriptor> {
        self.contracts.get(name)
    }

    /// List all registered contract names.
    pub fn task_names(&self) -> Vec<&str> {
        self.contracts.keys().map(|s| s.as_str()).collect()
    }

    /// Number of registered contracts.
    pub fn len(&self) -> usize {
        self.contracts.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.contracts.is_empty()
    }

    /// Validate input for a registered task.
    ///
    /// Returns an error if the task is unknown or if validation fails.
    pub fn validate_input(
        &self,
        task_name: &str,
        input: &serde_json::Value,
    ) -> Result<(), ContractViolation> {
        let desc = self
            .contracts
            .get(task_name)
            .ok_or_else(|| ContractViolation::unknown_task(task_name))?;
        let contract = TaskContract::new(desc.clone());
        contract.validate_input(input)
    }

    /// Validate output for a registered task.
    pub fn validate_output(
        &self,
        task_name: &str,
        output: &serde_json::Value,
    ) -> Result<(), ContractViolation> {
        let desc = self
            .contracts
            .get(task_name)
            .ok_or_else(|| ContractViolation::unknown_task(task_name))?;
        let contract = TaskContract::new(desc.clone());
        contract.validate_output(output)
    }
}

/// Error type for registry-level operations.
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("duplicate contract: {0}")]
    Duplicate(String),
    #[error("unknown task: {0}")]
    Unknown(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_descriptor(name: &str) -> TaskDescriptor {
        TaskDescriptor::new(
            name,
            format!("{} task", name),
            serde_json::json!({
                "type": "object",
                "required": ["path"],
                "properties": {
                    "path": {"type": "string"},
                    "encoding": {"type": "string"}
                }
            }),
            serde_json::json!({
                "type": "object",
                "required": ["content"],
                "properties": {
                    "content": {"type": "string"},
                    "size": {"type": "integer"}
                }
            }),
        )
    }

    // --- ContractViolation ---

    #[test]
    fn violation_display_no_field() {
        let v = ContractViolation::invalid_input("bad input");
        assert_eq!(v.to_string(), "[INVALID_INPUT] bad input");
    }

    #[test]
    fn violation_display_with_field() {
        let v = ContractViolation::invalid_input_field("path", "must be absolute");
        assert_eq!(
            v.to_string(),
            "[INVALID_INPUT] must be absolute (field: path)"
        );
    }

    #[test]
    fn violation_codes() {
        assert!(matches!(
            ContractViolation::unknown_task("foo").code,
            ViolationCode::UnknownTask
        ));
        assert!(matches!(
            ContractViolation::timeout("timed out").code,
            ViolationCode::Timeout
        ));
        assert!(matches!(
            ContractViolation::retries_exhausted("3 retries").code,
            ViolationCode::RetriesExhausted
        ));
    }

    // --- RetryPolicy ---

    #[test]
    fn retry_policy_fixed() {
        let policy = RetryPolicy {
            max_retries: 3,
            strategy: RetryStrategy::Fixed { interval_ms: 500 },
        };
        assert_eq!(policy.wait_duration(0), Duration::from_millis(500));
        assert_eq!(policy.wait_duration(2), Duration::from_millis(500));
    }

    #[test]
    fn retry_policy_exponential() {
        let policy = RetryPolicy {
            max_retries: 5,
            strategy: RetryStrategy::Exponential {
                base_ms: 100,
                max_ms: 10_000,
            },
        };
        assert_eq!(policy.wait_duration(0), Duration::from_millis(100));
        assert_eq!(policy.wait_duration(1), Duration::from_millis(200));
        assert_eq!(policy.wait_duration(2), Duration::from_millis(400));
        // Capped at max_ms.
        assert_eq!(policy.wait_duration(10), Duration::from_secs(10));
    }

    #[test]
    fn retry_policy_serde_roundtrip() {
        let policy = RetryPolicy {
            max_retries: 3,
            strategy: RetryStrategy::Exponential {
                base_ms: 200,
                max_ms: 30_000,
            },
        };
        let json = serde_json::to_string(&policy).unwrap();
        let decoded: RetryPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, policy);
    }

    // --- TaskDescriptor ---

    #[test]
    fn descriptor_builder() {
        let desc = sample_descriptor("file.read")
            .with_timeout(Duration::from_secs(30))
            .with_tag("fs")
            .with_tag("read");

        assert_eq!(desc.name, "file.read");
        assert_eq!(desc.timeout, Some(Duration::from_secs(30)));
        assert_eq!(desc.tags, vec!["fs", "read"]);
    }

    #[test]
    fn descriptor_serde_roundtrip() {
        let desc = sample_descriptor("test.task")
            .with_timeout(Duration::from_secs(10))
            .with_retry_policy(RetryPolicy {
                max_retries: 2,
                strategy: RetryStrategy::Fixed { interval_ms: 500 },
            });
        let json = serde_json::to_string(&desc).unwrap();
        let decoded: TaskDescriptor = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.name, desc.name);
        assert_eq!(decoded.description, desc.description);
        assert_eq!(decoded.timeout, desc.timeout);
        assert_eq!(decoded.retry_policy, desc.retry_policy);
    }

    // --- TaskContract validation ---

    #[test]
    fn validate_input_ok() {
        let contract = TaskContract::new(sample_descriptor("file.read"));
        let input = serde_json::json!({"path": "/tmp/test.txt"});
        assert!(contract.validate_input(&input).is_ok());
    }

    #[test]
    fn validate_input_missing_required() {
        let contract = TaskContract::new(sample_descriptor("file.read"));
        let input = serde_json::json!({"encoding": "utf-8"});
        let err = contract.validate_input(&input).unwrap_err();
        assert!(matches!(err.code, ViolationCode::InvalidInput));
        assert_eq!(err.field.as_deref(), Some("path"));
        assert!(err.message.contains("missing required"));
    }

    #[test]
    fn validate_input_wrong_type() {
        let contract = TaskContract::new(sample_descriptor("file.read"));
        let input = serde_json::json!({"path": 42});
        let err = contract.validate_input(&input).unwrap_err();
        assert!(matches!(err.code, ViolationCode::InvalidInput));
        assert_eq!(err.field.as_deref(), Some("path"));
        assert!(err.message.contains("expected type 'string'"));
    }

    #[test]
    fn validate_input_not_object() {
        let contract = TaskContract::new(sample_descriptor("file.read"));
        let input = serde_json::json!("just a string");
        let err = contract.validate_input(&input).unwrap_err();
        assert!(matches!(err.code, ViolationCode::InvalidInput));
        assert!(err.message.contains("must be a JSON object"));
    }

    #[test]
    fn validate_output_ok() {
        let contract = TaskContract::new(sample_descriptor("file.read"));
        let output = serde_json::json!({"content": "hello", "size": 5});
        assert!(contract.validate_output(&output).is_ok());
    }

    #[test]
    fn validate_output_missing_required() {
        let contract = TaskContract::new(sample_descriptor("file.read"));
        let output = serde_json::json!({"size": 0});
        let err = contract.validate_output(&output).unwrap_err();
        assert!(matches!(err.code, ViolationCode::InvalidOutput));
        assert_eq!(err.field.as_deref(), Some("content"));
    }

    #[test]
    fn validate_output_wrong_type() {
        let contract = TaskContract::new(sample_descriptor("file.read"));
        let output = serde_json::json!({"content": 123});
        let err = contract.validate_output(&output).unwrap_err();
        assert!(matches!(err.code, ViolationCode::InvalidOutput));
        assert!(err.message.contains("expected type 'string'"));
    }

    #[test]
    fn validate_non_object_schema_allows_any() {
        // Schema without "type": "object" should allow any value.
        let desc = TaskDescriptor::new(
            "free.form",
            "free-form task",
            serde_json::json!({}),
            serde_json::json!({}),
        );
        let contract = TaskContract::new(desc);
        assert!(contract
            .validate_input(&serde_json::json!("anything"))
            .is_ok());
        assert!(contract.validate_output(&serde_json::json!(42)).is_ok());
    }

    #[test]
    fn validate_with_custom_input_validator() {
        let desc = sample_descriptor("file.read");
        let contract = TaskContract::with_input_validator(desc, |input| {
            let path = input.get("path").and_then(|v| v.as_str()).unwrap_or("");
            if !path.starts_with('/') {
                return Err(ContractViolation::invalid_input_field(
                    "path",
                    "must be an absolute path",
                ));
            }
            Ok(())
        });

        assert!(contract
            .validate_input(&serde_json::json!({"path": "/tmp/x"}))
            .is_ok());
        let err = contract
            .validate_input(&serde_json::json!({"path": "relative/path"}))
            .unwrap_err();
        assert!(err.message.contains("absolute"));
    }

    #[test]
    fn validate_with_both_validators() {
        let desc = sample_descriptor("file.read");
        let contract = TaskContract::with_validators(
            desc,
            |input| {
                if input.get("path").is_none() {
                    return Err(ContractViolation::invalid_input("path required"));
                }
                Ok(())
            },
            |output| {
                if output.get("content").is_none() {
                    return Err(ContractViolation::invalid_output("content required"));
                }
                Ok(())
            },
        );

        assert!(contract
            .validate_input(&serde_json::json!({"path": "/a"}))
            .is_ok());
        assert!(contract
            .validate_output(&serde_json::json!({"content": "ok"}))
            .is_ok());
    }

    // --- TaskRegistry ---

    #[test]
    fn registry_register_and_get() {
        let mut reg = TaskRegistry::new();
        reg.register(sample_descriptor("file.read")).unwrap();
        reg.register(sample_descriptor("file.write")).unwrap();

        assert_eq!(reg.len(), 2);
        assert!(reg.get("file.read").is_some());
        assert!(reg.get("file.write").is_some());
        assert!(reg.get("file.delete").is_none());
    }

    #[test]
    fn registry_rejects_duplicate() {
        let mut reg = TaskRegistry::new();
        reg.register(sample_descriptor("file.read")).unwrap();
        let err = reg.register(sample_descriptor("file.read")).unwrap_err();
        assert!(matches!(err.code, ViolationCode::InvalidInput));
        assert!(err.message.contains("already registered"));
    }

    #[test]
    fn registry_validate_input_unknown_task() {
        let reg = TaskRegistry::new();
        let err = reg
            .validate_input("no.such.task", &serde_json::json!({}))
            .unwrap_err();
        assert!(matches!(err.code, ViolationCode::UnknownTask));
    }

    #[test]
    fn registry_validate_input_and_output() {
        let mut reg = TaskRegistry::new();
        reg.register(sample_descriptor("file.read")).unwrap();

        assert!(reg
            .validate_input("file.read", &serde_json::json!({"path": "/tmp/x"}))
            .is_ok());
        assert!(reg
            .validate_output("file.read", &serde_json::json!({"content": "ok"}))
            .is_ok());

        let err = reg
            .validate_input("file.read", &serde_json::json!({}))
            .unwrap_err();
        assert!(matches!(err.code, ViolationCode::InvalidInput));
    }

    #[test]
    fn registry_task_names() {
        let mut reg = TaskRegistry::new();
        reg.register(sample_descriptor("b.task")).unwrap();
        reg.register(sample_descriptor("a.task")).unwrap();

        let mut names = reg.task_names();
        names.sort_unstable();
        assert_eq!(names, vec!["a.task", "b.task"]);
    }

    #[test]
    fn registry_is_empty() {
        let reg = TaskRegistry::new();
        assert!(reg.is_empty());
    }

    // --- json_type_name helper ---

    #[test]
    fn json_type_name_variants() {
        assert_eq!(json_type_name(&serde_json::json!(null)), "null");
        assert_eq!(json_type_name(&serde_json::json!(true)), "boolean");
        assert_eq!(json_type_name(&serde_json::json!(42)), "number");
        assert_eq!(json_type_name(&serde_json::json!("hi")), "string");
        assert_eq!(json_type_name(&serde_json::json!([])), "array");
        assert_eq!(json_type_name(&serde_json::json!({})), "object");
    }
}
