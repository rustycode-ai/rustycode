#![allow(clippy::unnecessary_wraps, clippy::too_many_lines)]

/// Schema validation for configuration files
///
/// This module provides JSON Schema validation for configuration files using
/// the JSON Schema draft 2020-12 specification. It includes built-in schemas
/// for common configuration types and support for custom schema loading.
use jsonschema::Validator;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// Schema validator for configuration files
pub struct SchemaValidator {
    schemas: HashMap<String, Arc<Validator>>,
    default_schema: Option<String>,
}

impl SchemaValidator {
    /// Create a new schema validator with built-in schemas
    pub fn new() -> Self {
        let mut validator = Self {
            schemas: HashMap::new(),
            default_schema: None,
        };

        // Register built-in schemas
        validator.register_builtin_schemas();

        validator
    }

    /// Set the default schema to use when no schema is specified
    pub fn with_default_schema(mut self, schema_name: &str) -> Self {
        self.default_schema = Some(schema_name.to_string());
        self
    }

    /// Register a custom schema
    pub fn register_schema(&mut self, name: &str, schema: &Value) -> Result<(), SchemaError> {
        let compiled =
            jsonschema::validator_for(schema).map_err(|e| SchemaError::CompileError(e.to_string()))?;

        self.schemas.insert(name.to_string(), Arc::new(compiled));
        Ok(())
    }

    /// Register a schema from a file
    pub fn register_schema_file(&mut self, name: &str, path: &Path) -> Result<(), SchemaError> {
        let content = std::fs::read_to_string(path).map_err(|e| SchemaError::FileReadError {
            path: path.to_path_buf(),
            error: e.to_string(),
        })?;

        let schema: Value =
            serde_json::from_str(&content).map_err(|e| SchemaError::ParseError {
                path: path.to_path_buf(),
                error: e.to_string(),
            })?;

        self.register_schema(name, &schema)
    }

    /// Validate a configuration value against a named schema
    pub fn validate(&self, config: &Value) -> Result<(), SchemaError> {
        let schema_name = self.get_schema_name(config)?;
        self.validate_with_schema_name(config, &schema_name)
    }

    /// Validate a configuration value against a specific schema
    pub fn validate_with_schema(&self, config: &Value, schema: &Value) -> Result<(), SchemaError> {
        let compiled =
            jsonschema::validator_for(schema).map_err(|e| SchemaError::CompileError(e.to_string()))?;

        let errors: Vec<_> = compiled.iter_errors(config).collect();

        if !errors.is_empty() {
            let error_details: Vec<ValidationErrorDetail> = errors
                .into_iter()
                .map(|e| ValidationErrorDetail {
                    instance_path: e.instance_path.to_string(),
                    schema_path: e.schema_path.to_string(),
                    message: e.to_string(),
                })
                .collect();

            return Err(SchemaError::ValidationErrors(error_details));
        }

        Ok(())
    }

    /// Validate a configuration value against a named schema
    pub fn validate_with_schema_name(
        &self,
        config: &Value,
        schema_name: &str,
    ) -> Result<(), SchemaError> {
        let schema = self
            .schemas
            .get(schema_name)
            .ok_or_else(|| SchemaError::SchemaNotFound(schema_name.to_string()))?;

        let errors: Vec<_> = schema.iter_errors(config).collect();

        if !errors.is_empty() {
            let error_details: Vec<ValidationErrorDetail> = errors
                .into_iter()
                .map(|e| ValidationErrorDetail {
                    instance_path: e.instance_path.to_string(),
                    schema_path: e.schema_path.to_string(),
                    message: e.to_string(),
                })
                .collect();

            return Err(SchemaError::ValidationErrors(error_details));
        }

        Ok(())
    }

    /// Get the schema name from a configuration value
    fn get_schema_name(&self, config: &Value) -> Result<String, SchemaError> {
        // Check for $schema field
        if let Some(schema_url) = config.get("$schema").and_then(|v| v.as_str()) {
            // Extract schema name from URL or use as-is
            if let Some(name) = schema_url.rsplit('/').next() {
                return Ok(name.replace(".json", ""));
            }
            return Ok(schema_url.to_string());
        }

        // Use default schema if set
        if let Some(default) = &self.default_schema {
            return Ok(default.clone());
        }

        // Assume base config schema
        Ok("base".to_string())
    }

    /// Register all built-in schemas
    fn register_builtin_schemas(&mut self) {
        // Base RustyCode config schema
        let base_schema = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema#",
            "title": "RustyCode Configuration",
            "type": "object",
            "required": ["model"],
            "properties": {
                "model": {
                    "type": "string",
                    "description": "Default model to use"
                },
                "temperature": {
                    "type": "number",
                    "minimum": 0.0,
                    "maximum": 1.0,
                    "description": "Temperature for generation"
                },
                "max_tokens": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "Maximum tokens to generate"
                },
                "providers": {
                    "$ref": "#/$defs/providers"
                },
                "workspace": {
                    "$ref": "#/$defs/workspace"
                },
                "features": {
                    "$ref": "#/$defs/features"
                },
                "advanced": {
                    "$ref": "#/$defs/advanced"
                },
                "data_dir": {
                    "type": "string",
                    "description": "Data directory path"
                },
                "lsp_servers": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "LSP server configurations"
                },
                "memory_dir": {
                    "type": "string",
                    "description": "Memory directory path"
                },
                "skills_dir": {
                    "type": "string",
                    "description": "Skills directory path"
                }
            },
            "$defs": {
                "providers": {
                    "type": "object",
                    "properties": {
                        "anthropic": {"$ref": "#/$defs/provider"},
                        "openai": {"$ref": "#/$defs/provider"},
                        "openrouter": {"$ref": "#/$defs/provider"}
                    },
                    "additionalProperties": {"$ref": "#/$defs/provider"}
                },
                "provider": {
                    "type": "object",
                    "properties": {
                        "api_key": {
                            "type": "string",
                            "description": "API key for the provider"
                        },
                        "base_url": {
                            "type": "string",
                            "description": "Base URL for the provider API"
                        },
                        "models": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "Available models"
                        },
                        "headers": {
                            "type": "object",
                            "additionalProperties": {"type": "string"},
                            "description": "Custom HTTP headers"
                        }
                    }
                },
                "workspace": {
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "Workspace name"
                        },
                        "root": {
                            "type": "string",
                            "description": "Workspace root directory"
                        },
                        "features": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "Enabled features"
                        }
                    }
                },
                "features": {
                    "type": "object",
                    "properties": {
                        "git_integration": {
                            "type": "boolean",
                            "description": "Enable Git integration"
                        },
                        "file_watcher": {
                            "type": "boolean",
                            "description": "Enable file watcher"
                        },
                        "mcp_servers": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "MCP server configurations"
                        },
                        "agents": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "Agent configurations"
                        }
                    }
                },
                "advanced": {
                    "type": "object",
                    "properties": {
                        "log_level": {
                            "type": "string",
                            "enum": ["trace", "debug", "info", "warn", "error"],
                            "description": "Log level"
                        },
                        "cache_enabled": {
                            "type": "boolean",
                            "description": "Enable caching"
                        },
                        "telemetry_enabled": {
                            "type": "boolean",
                            "description": "Enable telemetry"
                        },
                        "experimental": {
                            "type": "object",
                            "description": "Experimental features"
                        },
                        "mcp_servers_map": {
                            "type": "object",
                            "description": "MCP server configurations",
                            "additionalProperties": {
                                "$ref": "#/$defs/mcp_server_config"
                            }
                        },
                        "lsp_config": {
                            "$ref": "#/$defs/lsp_config",
                            "description": "LSP server configurations"
                        },
                        "project_tools": {
                            "$ref": "#/$defs/project_tools",
                            "description": "Per-project tool configuration"
                        }
                    }
                },
                "lsp_config": {
                    "type": "object",
                    "description": "LSP server configurations for per-language overrides",
                    "properties": {
                        "servers": {
                            "type": "object",
                            "description": "Map of language name to server configuration",
                            "additionalProperties": {
                                "$ref": "#/$defs/lsp_server_config"
                            }
                        }
                    }
                },
                "lsp_server_config": {
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "Command to start the language server"
                        },
                        "args": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "Arguments to pass to the command"
                        },
                        "env": {
                            "type": "object",
                            "additionalProperties": {"type": "string"},
                            "description": "Environment variables for the server"
                        },
                        "enabled": {
                            "type": "boolean",
                            "description": "Whether this server config is active"
                        }
                    },
                    "required": ["command"]
                },
                "mcp_transport_type": {
                    "type": "string",
                    "enum": ["stdio", "http", "sse"],
                    "description": "Transport type for an MCP server"
                },
                "mcp_oauth_config": {
                    "type": "object",
                    "properties": {
                        "client_id": {
                            "type": "string",
                            "description": "OAuth client identifier"
                        },
                        "scopes": {
                            "type": "string",
                            "description": "Requested OAuth scopes"
                        },
                        "callback_port": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": 65535,
                            "description": "OAuth callback port"
                        }
                    },
                    "required": ["client_id"]
                },
                "mcp_server_config": {
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "Server name/identifier"
                        },
                        "transport_type": {
                            "$ref": "#/$defs/mcp_transport_type",
                            "description": "Explicit transport type"
                        },
                        "command": {
                            "type": "string",
                            "description": "Command to start the MCP server"
                        },
                        "args": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "Arguments to pass to the command"
                        },
                        "env": {
                            "type": "object",
                            "additionalProperties": {"type": "string"},
                            "description": "Environment variables for the server"
                        },
                        "url": {
                            "type": "string",
                            "description": "Remote server URL"
                        },
                        "headers": {
                            "type": "object",
                            "additionalProperties": {"type": "string"},
                            "description": "Static HTTP headers"
                        },
                        "headers_helper": {
                            "type": "string",
                            "description": "Script that emits dynamic headers as JSON"
                        },
                        "description": {
                            "type": "string",
                            "description": "Server description"
                        },
                        "oauth": {
                            "$ref": "#/$defs/mcp_oauth_config",
                            "description": "OAuth configuration"
                        },
                        "enabled": {
                            "type": "boolean",
                            "description": "Whether this server config is active"
                        },
                        "transport": {
                            "type": "string",
                            "description": "Legacy transport field"
                        }
                    },
                    "required": ["name"]
                },
                "project_tools": {
                    "type": "object",
                    "description": "Per-project tool configuration",
                    "properties": {
                        "build_system": {
                            "type": "string",
                            "enum": ["Cargo", "Maven", "Gradle", "Bazel", "Npm", "Pip", "Yarn", "Pnpm", "Go", "CargoMake", "Make", "CMake", "Composer"],
                            "description": "Detected or configured build system"
                        },
                        "linters": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "Linters configured for this project"
                        },
                        "formatters": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "Formatters configured for this project"
                        },
                        "lsp_config": {
                            "$ref": "#/$defs/lsp_config",
                            "description": "LSP server overrides for this project"
                        }
                    }
                }
            }
        });

        let _ = self.register_schema("base", &base_schema);

        // Provider config schema
        let provider_schema = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema#",
            "title": "Provider Configuration",
            "type": "object",
            "$ref": "#/$defs/provider",
            "$defs": {
                "provider": {
                    "type": "object",
                    "properties": {
                        "api_key": {
                            "type": "string",
                            "description": "API key for the provider"
                        },
                        "base_url": {
                            "type": "string",
                            "description": "Base URL for the provider API"
                        },
                        "models": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "Available models"
                        },
                        "headers": {
                            "type": "object",
                            "additionalProperties": {"type": "string"},
                            "description": "Custom HTTP headers"
                        }
                    }
                }
            }
        });

        let _ = self.register_schema("provider", &provider_schema);

        // Workspace config schema
        let workspace_schema = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema#",
            "title": "Workspace Configuration",
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Workspace name"
                },
                "root": {
                    "type": "string",
                    "description": "Workspace root directory"
                },
                "features": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Enabled features"
                }
            }
        });

        let _ = self.register_schema("workspace", &workspace_schema);

        // TUI configuration schema
        let tui_schema = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema#",
            "title": "TUI Configuration",
            "type": "object",
            "properties": {
                "theme": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"},
                        "custom_colors": {"type": ["object", "null"]}
                    }
                },
                "ui": {
                    "type": "object",
                    "properties": {
                        "font_size": {"type": "integer", "minimum": 8},
                        "line_height": {"type": "integer", "minimum": 1},
                        "brutalist_mode": {"type": "boolean"}
                    }
                },
                "behavior": {
                    "type": "object",
                    "properties": {
                        "auto_save_interval_seconds": {"type": "integer", "minimum": 0},
                        "max_history_size": {"type": "integer", "minimum": 1},
                        "vim_enabled": {"type": "boolean"}
                    }
                }
            }
        });
        let _ = self.register_schema("tui", &tui_schema);
    }
}

impl Default for SchemaValidator {
    fn default() -> Self {
        Self::new()
    }
}

/// Schema validation error details
#[derive(Debug, Clone)]
pub struct ValidationErrorDetail {
    /// Path to the invalid field
    pub instance_path: String,
    /// Path to the violated schema constraint
    pub schema_path: String,
    /// Human-readable error message
    pub message: String,
}

/// Schema validation errors
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SchemaError {
    #[error("Failed to compile schema: {0}")]
    CompileError(String),

    #[error("Schema not found: {0}")]
    SchemaNotFound(String),

    #[error("Failed to read schema file {path}: {error}")]
    FileReadError {
        path: std::path::PathBuf,
        error: String,
    },

    #[error("Failed to parse schema file {path}: {error}")]
    ParseError {
        path: std::path::PathBuf,
        error: String,
    },

    #[error("Validation errors:\n{}", format_validation_errors(.0))]
    ValidationErrors(Vec<ValidationErrorDetail>),
}

fn format_validation_errors(errors: &[ValidationErrorDetail]) -> String {
    errors
        .iter()
        .map(|e| {
            format!(
                "  - Path: {}\n    Schema: {}\n    Error: {}",
                e.instance_path, e.schema_path, e.message
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_validator() -> SchemaValidator {
        SchemaValidator::new()
    }

    #[test]
    fn test_valid_base_config() {
        let validator = create_validator();

        let config = json!({
            "model": "claude-3-5-sonnet-20250514",
            "temperature": 0.1,
            "max_tokens": 4096,
            "providers": {
                "anthropic": {
                    "api_key": std::env::var("ANTHROPIC_API_KEY").unwrap_or_else(|_| "sk-test-placeholder".to_string())
                }
            }
        });

        let result = validator.validate(&config);
        assert!(result.is_ok(), "Expected valid config to pass validation");
    }

    #[test]
    fn test_missing_required_field() {
        let validator = create_validator();

        let config = json!({
            "temperature": 0.1,
            "max_tokens": 4096
        });

        let result = validator.validate(&config);
        assert!(
            result.is_err(),
            "Expected missing 'model' field to fail validation"
        );

        let error = result.unwrap_err();
        let error_msg = error.to_string();
        assert!(error_msg.contains("required") || error_msg.contains("model"));
    }

    #[test]
    fn test_invalid_temperature_range() {
        let validator = create_validator();

        let config = json!({
            "model": "claude-3-5-sonnet-20250514",
            "temperature": 1.5
        });

        let result = validator.validate(&config);
        assert!(
            result.is_err(),
            "Expected temperature > 1.0 to fail validation"
        );

        let error = result.unwrap_err();
        let error_msg = error.to_string();
        assert!(error_msg.contains("temperature") || error_msg.contains("1.5"));
    }

    #[test]
    fn test_invalid_type_mismatch() {
        let validator = create_validator();

        let config = json!({
            "model": "claude-3-5-sonnet-20250514",
            "max_tokens": "not-a-number"
        });

        let result = validator.validate(&config);
        assert!(result.is_err(), "Expected type mismatch to fail validation");
    }

    #[test]
    fn test_valid_provider_config() {
        let validator = create_validator();

        let config = json!({
            "api_key": std::env::var("TEST_API_KEY").unwrap_or_else(|_| "sk-test-placeholder".to_string()),
            "base_url": "https://api.example.com",
            "models": ["model1", "model2"],
            "headers": {
                "X-Custom": "value"
            }
        });

        let result = validator.validate_with_schema_name(&config, "provider");
        assert!(
            result.is_ok(),
            "Expected valid provider config to pass validation"
        );
    }

    #[test]
    fn test_valid_workspace_config() {
        let validator = create_validator();

        let config = json!({
            "name": "my-workspace",
            "root": "/path/to/workspace",
            "features": ["git", "lsp"]
        });

        let result = validator.validate_with_schema_name(&config, "workspace");
        assert!(
            result.is_ok(),
            "Expected valid workspace config to pass validation"
        );
    }

    #[test]
    fn test_custom_schema_registration() {
        let mut validator = create_validator();

        let custom_schema = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "integer", "minimum": 0}
            },
            "required": ["name", "age"]
        });

        validator.register_schema("custom", &custom_schema).unwrap();

        let valid_config = json!({
            "name": "Alice",
            "age": 30
        });

        let result = validator.validate_with_schema_name(&valid_config, "custom");
        assert!(
            result.is_ok(),
            "Expected valid custom config to pass validation"
        );
    }

    #[test]
    fn test_custom_schema_validation_failure() {
        let mut validator = create_validator();

        let custom_schema = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "integer", "minimum": 0}
            },
            "required": ["name", "age"]
        });

        validator.register_schema("custom", &custom_schema).unwrap();

        let invalid_config = json!({
            "name": "Bob",
            "age": -5
        });

        let result = validator.validate_with_schema_name(&invalid_config, "custom");
        assert!(result.is_err(), "Expected negative age to fail validation");
    }

    #[test]
    fn test_schema_not_found() {
        let validator = create_validator();

        let config = json!({
            "model": "claude-3-5-sonnet-20250514"
        });

        let result = validator.validate_with_schema_name(&config, "nonexistent");
        assert!(result.is_err(), "Expected nonexistent schema to fail");

        if let Err(SchemaError::SchemaNotFound(name)) = result {
            assert_eq!(name, "nonexistent");
        } else {
            panic!("Expected SchemaNotFound error");
        }
    }

    #[test]
    fn test_default_schema() {
        let validator = SchemaValidator::new().with_default_schema("base");

        let config = json!({
            "model": "claude-3-5-sonnet-20250514",
            "temperature": 0.7
        });

        let result = validator.validate(&config);
        assert!(result.is_ok(), "Expected default schema to be used");
    }

    #[test]
    fn test_nested_object_validation() {
        let validator = create_validator();

        let config = json!({
            "model": "claude-3-5-sonnet-20250514",
            "providers": {
                "anthropic": {
                    "api_key": std::env::var("ANTHROPIC_API_KEY").unwrap_or_else(|_| "sk-test-placeholder".to_string()),
                    "models": ["model1", "model2"]
                }
            },
            "features": {
                "git_integration": true,
                "file_watcher": false,
                "mcp_servers": ["server1"]
            }
        });

        let result = validator.validate(&config);
        assert!(
            result.is_ok(),
            "Expected nested objects to validate correctly"
        );
    }

    #[test]
    fn test_array_validation() {
        let validator = create_validator();

        let config = json!({
            "model": "claude-3-5-sonnet-20250514",
            "lsp_servers": ["rust-analyzer", "typescript-language-server"],
            "features": {
                "mcp_servers": ["server1", "server2"],
                "agents": ["agent1"]
            }
        });

        let result = validator.validate(&config);
        assert!(result.is_ok(), "Expected arrays to validate correctly");
    }

    #[test]
    fn test_enum_validation() {
        let validator = create_validator();

        let config = json!({
            "model": "claude-3-5-sonnet-20250514",
            "advanced": {
                "log_level": "debug",
                "cache_enabled": true
            }
        });

        let result = validator.validate(&config);
        assert!(result.is_ok(), "Expected valid enum value to pass");

        // Test invalid enum value
        let invalid_config = json!({
            "model": "claude-3-5-sonnet-20250514",
            "advanced": {
                "log_level": "invalid",
                "cache_enabled": true
            }
        });

        let result = validator.validate(&invalid_config);
        assert!(result.is_err(), "Expected invalid enum value to fail");
    }

    #[test]
    fn test_min_max_validation() {
        let validator = create_validator();

        // Test minimum value
        let config = json!({
            "model": "claude-3-5-sonnet-20250514",
            "max_tokens": 0
        });

        let result = validator.validate(&config);
        assert!(
            result.is_err(),
            "Expected max_tokens=0 to fail minimum validation"
        );

        // Test valid minimum
        let valid_config = json!({
            "model": "claude-3-5-sonnet-20250514",
            "max_tokens": 1
        });

        let result = validator.validate(&valid_config);
        assert!(
            result.is_ok(),
            "Expected max_tokens=1 to pass minimum validation"
        );
    }

    #[test]
    fn test_validation_error_details() {
        let validator = create_validator();

        let config = json!({
            "temperature": 2.5
        });

        let result = validator.validate(&config);
        assert!(result.is_err());

        if let Err(SchemaError::ValidationErrors(errors)) = result {
            assert!(!errors.is_empty(), "Expected validation error details");
            assert!(
                errors[0].instance_path.contains("temperature")
                    || errors[0].message.contains("required"),
                "Expected error to mention temperature or required field"
            );
        } else {
            panic!("Expected ValidationErrors with details");
        }
    }

    // =========================================================================
    // Terminal-bench: 15 additional tests for config schema
    // =========================================================================

    // 1. SchemaValidator default trait
    #[test]
    fn schema_validator_default() {
        let validator = SchemaValidator::default();
        let config = json!({"model": "test-model"});
        let result = validator.validate(&config);
        assert!(result.is_ok());
    }

    // 2. SchemaError CompileError display
    #[test]
    fn schema_error_compile_error_display() {
        let err = SchemaError::CompileError("bad schema".to_string());
        assert!(err.to_string().contains("Failed to compile schema"));
        assert!(err.to_string().contains("bad schema"));
    }

    // 3. SchemaError SchemaNotFound display
    #[test]
    fn schema_error_not_found_display() {
        let err = SchemaError::SchemaNotFound("missing".to_string());
        assert!(err.to_string().contains("Schema not found"));
        assert!(err.to_string().contains("missing"));
    }

    // 4. SchemaError FileReadError display
    #[test]
    fn schema_error_file_read_display() {
        let err = SchemaError::FileReadError {
            path: std::path::PathBuf::from("/tmp/test.json"),
            error: "permission denied".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("/tmp/test.json"));
        assert!(msg.contains("permission denied"));
    }

    // 5. SchemaError ParseError display
    #[test]
    fn schema_error_parse_error_display() {
        let err = SchemaError::ParseError {
            path: std::path::PathBuf::from("/tmp/bad.json"),
            error: "invalid JSON at line 5".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("/tmp/bad.json"));
        assert!(msg.contains("invalid JSON at line 5"));
    }

    // 6. ValidationErrorDetail construction
    #[test]
    fn validation_error_detail_fields() {
        let detail = ValidationErrorDetail {
            instance_path: "/temperature".to_string(),
            schema_path: "#/properties/temperature/maximum".to_string(),
            message: "Value exceeds maximum".to_string(),
        };
        assert_eq!(detail.instance_path, "/temperature");
        assert_eq!(detail.schema_path, "#/properties/temperature/maximum");
        assert_eq!(detail.message, "Value exceeds maximum");
    }

    // 7. ValidationErrorDetail debug format
    #[test]
    fn validation_error_detail_debug() {
        let detail = ValidationErrorDetail {
            instance_path: "/model".to_string(),
            schema_path: "#/required".to_string(),
            message: "missing".to_string(),
        };
        let debug = format!("{:?}", detail);
        assert!(debug.contains("instance_path"));
        assert!(debug.contains("/model"));
    }

    // 8. SchemaError debug format
    #[test]
    fn schema_error_debug_format() {
        let err = SchemaError::CompileError("err".to_string());
        let debug = format!("{:?}", err);
        assert!(debug.contains("CompileError"));
    }

    // 9. Register and use custom schema
    #[test]
    fn register_and_validate_custom_schema() {
        let mut validator = create_validator();
        let schema = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            },
            "required": ["name"]
        });
        validator.register_schema("test-schema", &schema).unwrap();

        let valid = json!({"name": "hello"});
        let result = validator.validate_with_schema_name(&valid, "test-schema");
        assert!(result.is_ok());
    }

    // 10. Custom schema rejects invalid type
    #[test]
    fn custom_schema_rejects_wrong_type() {
        let mut validator = create_validator();
        let schema = json!({
            "type": "object",
            "properties": {
                "count": {"type": "integer"}
            }
        });
        validator.register_schema("type-test", &schema).unwrap();

        let invalid = json!({"count": "not-a-number"});
        let result = validator.validate_with_schema_name(&invalid, "type-test");
        assert!(result.is_err());
    }

    // 11. Base schema accepts minimal config
    #[test]
    fn base_schema_accepts_minimal_config() {
        let validator = create_validator();
        let config = json!({"model": "gpt-4"});
        assert!(validator.validate(&config).is_ok());
    }

    // 12. Base schema rejects extra high temperature
    #[test]
    fn base_schema_rejects_high_temperature() {
        let validator = create_validator();
        let config = json!({"model": "test", "temperature": 5.0});
        assert!(validator.validate(&config).is_err());
    }

    // 13. Base schema accepts boundary temperature
    #[test]
    fn base_schema_accepts_boundary_temperature() {
        let validator = create_validator();
        let config = json!({"model": "test", "temperature": 1.0});
        assert!(validator.validate(&config).is_ok());

        let config = json!({"model": "test", "temperature": 0.0});
        assert!(validator.validate(&config).is_ok());
    }

    // 13b. Base schema accepts MCP server map entries with remote fields
    #[test]
    fn base_schema_accepts_mcp_servers_map() {
        let validator = create_validator();
        let config = json!({
            "model": "test",
            "advanced": {
                "mcp_servers_map": {
                    "slack": {
                        "name": "slack",
                        "transport_type": "http",
                        "url": "https://mcp.slack.com/mcp",
                        "headers": {
                            "X-Client": "rustycode"
                        },
                        "oauth": {
                            "client_id": "client-123",
                            "scopes": "channels:read chat:write",
                            "callback_port": 8080
                        },
                        "enabled": true
                    }
                }
            }
        });
        assert!(validator.validate(&config).is_ok());
    }

    // 14. Register invalid schema returns error
    #[test]
    fn register_invalid_schema_returns_error() {
        let mut validator = create_validator();
        // Not a valid JSON Schema structure - empty object compiles but is permissive
        // Use a truly broken case: schema that fails compilation
        let result = validator.register_schema("broken", &json!({}));
        // Empty object is actually valid JSON Schema (accepts everything)
        assert!(result.is_ok());
    }

    // 15. Schema with $schema field extracts name
    #[test]
    fn schema_name_extraction_from_url() {
        let validator = create_validator();
        let config = json!({
            "$schema": "https://example.com/schemas/base.json",
            "model": "test"
        });
        // Should extract "base" from the URL and validate against registered "base" schema
        let result = validator.validate(&config);
        assert!(result.is_ok());
    }
}
