//! Plugin configuration and credential management
//!
//! Handles per-plugin configuration loading, environment variable substitution,
//! and secure credential storage.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::fmt;
use std::ops::Deref;

use crate::error::PluginError;

/// A configuration value that may be sensitive (e.g., API keys)
///
/// Sensitive values are never logged, displayed, or debugged.
#[derive(Clone, Serialize, Deserialize)]
pub struct SensitiveValue<T: Serialize> {
    value: T,
    is_sensitive: bool,
}

impl<T: Serialize> SensitiveValue<T> {
    /// Create a new sensitive value
    pub const fn new(value: T, is_sensitive: bool) -> Self {
        Self {
            value,
            is_sensitive,
        }
    }

    /// Create a sensitive value (always marked as sensitive)
    pub const fn sensitive(value: T) -> Self {
        Self {
            value,
            is_sensitive: true,
        }
    }

    /// Create a non-sensitive value
    pub const fn public(value: T) -> Self {
        Self {
            value,
            is_sensitive: false,
        }
    }

    /// Check if this value is marked as sensitive
    pub const fn is_sensitive(&self) -> bool {
        self.is_sensitive
    }

    /// Get the underlying value
    pub const fn value(&self) -> &T {
        &self.value
    }

    /// Consume self and return the inner value
    pub fn into_inner(self) -> T {
        self.value
    }
}

impl<T: Serialize> Deref for SensitiveValue<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T: Serialize> fmt::Debug for SensitiveValue<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_sensitive {
            f.write_str("SensitiveValue([REDACTED])")
        } else {
            f.debug_struct("SensitiveValue")
                .field("is_sensitive", &self.is_sensitive)
                .finish()
        }
    }
}

impl<T: Serialize> fmt::Display for SensitiveValue<T>
where
    T: ToString,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_sensitive {
            f.write_str("[REDACTED]")
        } else {
            f.write_str(&self.value.to_string())
        }
    }
}

/// Configuration for a single plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    /// Plugin name
    pub name: String,

    /// Configuration values (merged from defaults, file, environment)
    pub values: HashMap<String, ConfigValue>,

    /// Sensitive field names (for masking in logs)
    sensitive_fields: Vec<String>,
}

impl PluginConfig {
    /// Create a new empty plugin configuration
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            values: HashMap::new(),
            sensitive_fields: vec![],
        }
    }

    /// Add a configuration value
    pub fn set(&mut self, key: impl Into<String>, value: ConfigValue) {
        let key = key.into();
        self.values.insert(key, value);
    }

    /// Mark a field as sensitive
    pub fn mark_sensitive(&mut self, field: impl Into<String>) {
        self.sensitive_fields.push(field.into());
    }

    /// Get a configuration value
    pub fn get(&self, key: &str) -> Option<&ConfigValue> {
        self.values.get(key)
    }

    /// Get a string value
    pub fn get_string(&self, key: &str) -> Option<String> {
        self.get(key).and_then(ConfigValue::as_string)
    }

    /// Get an integer value
    pub fn get_int(&self, key: &str) -> Option<i64> {
        self.get(key).and_then(ConfigValue::as_int)
    }

    /// Get a boolean value
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.get(key).and_then(ConfigValue::as_bool)
    }

    /// Check if a field is marked as sensitive
    pub fn is_sensitive(&self, field: &str) -> bool {
        self.sensitive_fields.contains(&field.to_string())
    }

    /// Get configuration value, masking sensitive values for logging
    pub fn get_for_logging(&self, key: &str) -> Option<String> {
        self.get(key).map(|v| {
            if self.is_sensitive(key) {
                "[REDACTED]".to_string()
            } else {
                v.to_string()
            }
        })
    }

    /// Perform environment variable substitution
    ///
    /// Replaces ${`VAR_NAME`} with environment variable values.
    /// Returns error if a required env var is not found.
    pub fn substitute_env_vars(&mut self, allow_missing: bool) -> Result<(), PluginError> {
        for value in self.values.values_mut() {
            value.substitute_env_vars(allow_missing)?;
        }
        Ok(())
    }

    /// Create a builder for constructing configurations
    pub fn builder(name: impl Into<String>) -> ConfigBuilder {
        ConfigBuilder::new(name)
    }
}

/// A configuration value that can be various types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
#[non_exhaustive]
pub enum ConfigValue {
    /// String value
    String(String),
    /// Integer value
    Int(i64),
    /// Boolean value
    Bool(bool),
    /// Floating point value
    Float(f64),
    /// JSON object
    Object(serde_json::Map<String, JsonValue>),
    /// Array of values
    Array(Vec<JsonValue>),
}

impl ConfigValue {
    /// Try to get as string
    pub fn as_string(&self) -> Option<String> {
        match self {
            Self::String(s) => Some(s.clone()),
            _ => None,
        }
    }

    /// Try to get as integer
    pub const fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(i) => Some(*i),
            _ => None,
        }
    }

    /// Try to get as boolean
    pub const fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Try to get as float
    pub const fn as_float(&self) -> Option<f64> {
        match self {
            Self::Float(f) => Some(*f),
            _ => None,
        }
    }

    /// Perform environment variable substitution on string values
    pub fn substitute_env_vars(&mut self, allow_missing: bool) -> Result<(), PluginError> {
        if let Self::String(s) = self {
            *s = Self::substitute_string(s, allow_missing)?;
        }
        Ok(())
    }

    /// Substitute ${VAR} patterns in a string
    fn substitute_string(s: &str, allow_missing: bool) -> Result<String, PluginError> {
        let mut result = s.to_string();

        // Simple regex-like substitution for ${VAR_NAME}
        while let Some(start) = result.find("${") {
            if let Some(end) = result[start..].find('}') {
                let end = start + end;
                let var_name = &result[start + 2..end];

                if let Ok(v) = std::env::var(var_name) {
                    result.replace_range(start..=end, &v);
                } else {
                    if allow_missing {
                        // Leave the ${VAR} placeholder as-is
                        // Recursively process the rest of the string after this placeholder
                        let rest = &result[end + 1..];
                        if !rest.is_empty() {
                            if let Ok(processed_rest) = Self::substitute_string(rest, allow_missing)
                            {
                                result.truncate(end + 1);
                                result.push_str(&processed_rest);
                            }
                        }
                        // We're done with this string
                        break;
                    }
                    return Err(PluginError::configuration_error(format!(
                        "environment variable {var_name} not found"
                    )));
                }
            } else {
                break;
            }
        }

        Ok(result)
    }
}

impl fmt::Display for ConfigValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::String(s) => write!(f, "{s}"),
            Self::Int(i) => write!(f, "{i}"),
            Self::Bool(b) => write!(f, "{b}"),
            Self::Float(fl) => write!(f, "{fl}"),
            Self::Object(_) => write!(f, "[object]"),
            Self::Array(_) => write!(f, "[array]"),
        }
    }
}

impl From<String> for ConfigValue {
    fn from(s: String) -> Self {
        Self::String(s)
    }
}

impl From<&str> for ConfigValue {
    fn from(s: &str) -> Self {
        Self::String(s.to_string())
    }
}

impl From<i64> for ConfigValue {
    fn from(i: i64) -> Self {
        Self::Int(i)
    }
}

impl From<bool> for ConfigValue {
    fn from(b: bool) -> Self {
        Self::Bool(b)
    }
}

impl From<f64> for ConfigValue {
    fn from(f: f64) -> Self {
        Self::Float(f)
    }
}

/// Builder for constructing plugin configurations
pub struct ConfigBuilder {
    config: PluginConfig,
}

impl ConfigBuilder {
    /// Create a new configuration builder
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            config: PluginConfig::new(name),
        }
    }

    /// Add a configuration value
    pub fn with_value(mut self, key: impl Into<String>, value: impl Into<ConfigValue>) -> Self {
        self.config.set(key, value.into());
        self
    }

    /// Mark a field as sensitive
    pub fn mark_sensitive(mut self, field: impl Into<String>) -> Self {
        self.config.mark_sensitive(field);
        self
    }

    /// Build the configuration
    pub fn build(self) -> PluginConfig {
        self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    fn test_sensitive_value_new() {
        let value = SensitiveValue::new("secret".to_string(), true);
        assert!(value.is_sensitive());
        assert_eq!(value.value(), "secret");
    }

    #[test]
    fn test_sensitive_value_sensitive() {
        let value = SensitiveValue::sensitive("api_key".to_string());
        assert!(value.is_sensitive());
    }

    #[test]
    fn test_sensitive_value_public() {
        let value = SensitiveValue::public("public_data".to_string());
        assert!(!value.is_sensitive());
    }

    #[test]
    fn test_sensitive_value_display() {
        let sensitive = SensitiveValue::sensitive("secret".to_string());
        assert_eq!(sensitive.to_string(), "[REDACTED]");

        let public = SensitiveValue::public("public".to_string());
        assert_eq!(public.to_string(), "public");
    }

    #[test]
    fn test_sensitive_value_debug() {
        let sensitive = SensitiveValue::sensitive("secret".to_string());
        let debug_str = format!("{:?}", sensitive);
        assert!(debug_str.contains("REDACTED"));

        let public = SensitiveValue::public("public".to_string());
        let debug_str = format!("{:?}", public);
        assert!(!debug_str.contains("REDACTED"));
    }

    #[test]
    fn test_plugin_config_new() {
        let config = PluginConfig::new("test_plugin");
        assert_eq!(config.name, "test_plugin");
        assert!(config.values.is_empty());
    }

    #[test]
    fn test_plugin_config_set_get() {
        let mut config = PluginConfig::new("test_plugin");
        config.set("key1", ConfigValue::String("value1".to_string()));
        config.set("key2", ConfigValue::Int(42));

        assert_eq!(config.get_string("key1"), Some("value1".to_string()));
        assert_eq!(config.get_int("key2"), Some(42));
    }

    #[test]
    fn test_plugin_config_sensitive_fields() {
        let mut config = PluginConfig::new("test_plugin");
        config.set("api_key", ConfigValue::String("secret".to_string()));
        config.mark_sensitive("api_key");

        assert!(config.is_sensitive("api_key"));
        assert!(!config.is_sensitive("other_field"));

        assert_eq!(
            config.get_for_logging("api_key"),
            Some("[REDACTED]".to_string())
        );
        assert_eq!(config.get_for_logging("other_key"), None);
    }

    #[test]
    fn test_plugin_config_builder() {
        let config = PluginConfig::builder("test_plugin")
            .with_value("key1", "value1")
            .with_value("key2", 42)
            .mark_sensitive("key1")
            .build();

        assert_eq!(config.name, "test_plugin");
        assert_eq!(config.get_string("key1"), Some("value1".to_string()));
        assert_eq!(config.get_int("key2"), Some(42));
        assert!(config.is_sensitive("key1"));
    }

    #[test]
    fn test_config_value_conversions() {
        assert_eq!(
            ConfigValue::from("text".to_string()).as_string(),
            Some("text".to_string())
        );
        assert_eq!(ConfigValue::from(42i64).as_int(), Some(42));
        assert_eq!(ConfigValue::from(true).as_bool(), Some(true));
        assert_eq!(ConfigValue::from(2.5).as_float(), Some(2.5));
    }

    #[test]
    fn test_config_value_display() {
        assert_eq!(ConfigValue::String("text".to_string()).to_string(), "text");
        assert_eq!(ConfigValue::Int(42).to_string(), "42");
        assert_eq!(ConfigValue::Bool(true).to_string(), "true");
        assert_eq!(ConfigValue::Float(2.5).to_string(), "2.5");
    }

    #[test]
    #[serial]
    fn test_substitute_env_vars() {
        std::env::set_var("TEST_VAR", "test_value");

        let mut config = PluginConfig::new("test_plugin");
        config.set(
            "key",
            ConfigValue::String("prefix_${TEST_VAR}_suffix".to_string()),
        );
        assert!(config.substitute_env_vars(false).is_ok());

        assert_eq!(
            config.get_string("key"),
            Some("prefix_test_value_suffix".to_string())
        );

        std::env::remove_var("TEST_VAR");
    }

    #[test]
    fn test_substitute_env_vars_missing() {
        let mut config = PluginConfig::new("test_plugin");
        config.set("key", ConfigValue::String("${MISSING_VAR}".to_string()));

        let result = config.substitute_env_vars(false);
        assert!(result.is_err());
    }

    #[test]
    fn test_substitute_env_vars_allow_missing() {
        let mut config = PluginConfig::new("test_plugin");
        config.set(
            "key",
            ConfigValue::String("prefix_${MISSING_VAR}_suffix".to_string()),
        );

        let result = config.substitute_env_vars(true);
        assert!(result.is_ok());
        // With allow_missing=true, missing vars should be left as placeholders
        assert_eq!(
            config.get_string("key"),
            Some("prefix_${MISSING_VAR}_suffix".to_string())
        );
    }

    #[test]
    fn test_config_value_types() {
        let string_val = ConfigValue::String("hello".to_string());
        assert_eq!(string_val.as_string(), Some("hello".to_string()));
        assert_eq!(string_val.as_int(), None);

        let int_val = ConfigValue::Int(123);
        assert_eq!(int_val.as_int(), Some(123));
        assert_eq!(int_val.as_string(), None);

        let bool_val = ConfigValue::Bool(true);
        assert_eq!(bool_val.as_bool(), Some(true));
        assert_eq!(bool_val.as_int(), None);
    }

    #[test]
    fn test_plugin_config_multiple_sensitive_fields() {
        let config = PluginConfig::builder("test_plugin")
            .with_value("api_key", "secret_key")
            .with_value("db_password", "secret_pass")
            .with_value("username", "user123")
            .mark_sensitive("api_key")
            .mark_sensitive("db_password")
            .build();

        assert!(config.is_sensitive("api_key"));
        assert!(config.is_sensitive("db_password"));
        assert!(!config.is_sensitive("username"));

        assert_eq!(
            config.get_for_logging("api_key"),
            Some("[REDACTED]".to_string())
        );
        assert_eq!(
            config.get_for_logging("username"),
            Some("user123".to_string())
        );
    }

    #[test]
    #[serial]
    fn test_substitute_env_vars_multiple() {
        std::env::set_var("VAR1", "value1");
        std::env::set_var("VAR2", "value2");

        let mut config = PluginConfig::new("test_plugin");
        config.set("key", ConfigValue::String("${VAR1}:${VAR2}".to_string()));
        assert!(config.substitute_env_vars(false).is_ok());

        assert_eq!(config.get_string("key"), Some("value1:value2".to_string()));

        std::env::remove_var("VAR1");
        std::env::remove_var("VAR2");
    }
}
