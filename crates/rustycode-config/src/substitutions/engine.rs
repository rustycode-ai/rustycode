#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::uninlined_format_args
    )
)]
// RustyCode Substitution Engine
//
// Handles {env:VAR} and {file:path} substitutions in configuration.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

pub struct SubstitutionEngine {
    cache: HashMap<String, CachedValue>,
    recursion_limit: usize,
    /// Security: restrict file reads to this directory (None = default config dir)
    allowed_base: Option<PathBuf>,
}

#[derive(Clone)]
struct CachedValue {
    value: String,
    timestamp: SystemTime,
    ttl: Option<Duration>,
}

impl Default for SubstitutionEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl SubstitutionEngine {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            recursion_limit: 10,
            allowed_base: None,
        }
    }

    /// Set the allowed base directory for file substitutions.
    /// If None, uses the default ~/.config/rustycode directory.
    /// This is useful for testing.
    pub fn with_allowed_base(mut self, base: PathBuf) -> Self {
        self.allowed_base = Some(base);
        self
    }

    pub fn process(&mut self, input: &str) -> Result<String, SubstitutionError> {
        self.process_with_depth(input, 0)
    }

    fn process_with_depth(
        &mut self,
        input: &str,
        depth: usize,
    ) -> Result<String, SubstitutionError> {
        if depth >= self.recursion_limit {
            return Err(SubstitutionError::RecursionLimitExceeded);
        }

        // Pre-allocate result string with estimated capacity
        let mut result = String::with_capacity(input.len());
        let mut chars = input.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '{' {
                // Check for escaped {{
                if chars.peek() == Some(&'{') {
                    chars.next();
                    result.push('{');
                    continue;
                }

                // Extract substitution
                let substitution = self.extract_substitution(&mut chars)?;

                // Resolve substitution recursively (support nested substitutions)
                let resolved = self.resolve_substitution(&substitution, depth)?;

                result.push_str(&resolved);
            } else {
                result.push(ch);
            }
        }

        Ok(result)
    }

    #[allow(clippy::unused_self)]
    fn extract_substitution(
        &self,
        chars: &mut std::iter::Peekable<std::str::Chars>,
    ) -> Result<String, SubstitutionError> {
        // Pre-allocate substitution string with reasonable capacity
        let mut substitution = String::with_capacity(64);
        let mut brace_depth = 1;

        for ch in chars.by_ref() {
            match ch {
                '{' => {
                    brace_depth += 1;
                    substitution.push(ch);
                }
                '}' => {
                    brace_depth -= 1;
                    if brace_depth == 0 {
                        return Ok(substitution);
                    }
                    substitution.push(ch);
                }
                _ => substitution.push(ch),
            }
        }

        Err(SubstitutionError::UnterminatedSubstitution)
    }

    fn resolve_substitution(
        &mut self,
        substitution: &str,
        depth: usize,
    ) -> Result<String, SubstitutionError> {
        // Parse substitution: allow either `{kind:value}` or shorthand `{kind}`
        let (raw_kind, raw_value) = substitution.find(':').map_or((substitution, ""), |pos| {
            (&substitution[..pos], &substitution[pos + 1..])
        });

        // Trim whitespace around kind/value to be permissive (e.g., `{ current_model }`)
        let kind = raw_kind.trim();
        let value = raw_value.trim();

        // Normalize kind: strip surrounding quotes, replace '-' with '_' and lowercase
        let norm_kind = kind
            .trim_matches(|c| c == '"' || c == '\'')
            .replace('-', "_")
            .to_lowercase();

        // If the value contains nested substitutions, process them first
        let processed_value = if value.contains('{') {
            // Increase depth to avoid infinite recursion
            self.process_with_depth(value, depth + 1)?
        } else {
            value.to_string()
        };

        match norm_kind.as_str() {
            "env" => self.resolve_env(&processed_value),
            "file" => self.resolve_file(&processed_value),
            "default" => Ok(processed_value),
            // Backward-compatible helpers used in config templates/tests
            "current_model" => std::env::var("CURRENT_MODEL")
                .map_or_else(|_| Ok("claude-sonnet-4-6".to_string()), Ok),
            // Allow shorthand: if someone writes `{kind}` treat as kind with empty value
            _ => Err(SubstitutionError::UnknownKind(norm_kind)),
        }
    }

    #[allow(clippy::unused_self)]
    fn resolve_env(&self, var_name: &str) -> Result<String, SubstitutionError> {
        std::env::var(var_name).map_err(|_| SubstitutionError::EnvVarNotFound(var_name.to_string()))
    }

    fn resolve_file(&mut self, path: &str) -> Result<String, SubstitutionError> {
        // Expand ~
        let expanded = self.expand_tilde(path)?;

        // SECURITY: Restrict file reads to designated config directory
        // This prevents reading arbitrary files like /etc/passwd, ~/.ssh/id_rsa, etc.
        let allowed_base = self.allowed_base.clone().unwrap_or_else(|| {
            dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("rustycode")
        });

        // Ensure the allowed base directory exists before canonicalizing
        // (canonicalize fails if the path doesn't exist on disk)
        if !allowed_base.exists() {
            if let Err(e) = std::fs::create_dir_all(&allowed_base) {
                return Err(SubstitutionError::SecurityError(format!(
                    "Cannot create allowed base directory '{}': {e}",
                    allowed_base.display()
                )));
            }
        }

        // Canonicalize both paths to resolve symlinks and prevent TOCTOU attacks
        let canonical_path = std::fs::canonicalize(&expanded)
            .map_err(|e| SubstitutionError::FileReadError(expanded.clone(), e.to_string()))?;

        let canonical_allowed = std::fs::canonicalize(&allowed_base).map_err(|e| {
            SubstitutionError::SecurityError(format!(
                "Cannot canonicalize allowed base directory '{}': {}. \
                 Refusing to proceed with non-canonical path to prevent symlink attacks.",
                allowed_base.display(),
                e
            ))
        })?;

        // Verify the file is within the allowed directory
        if !canonical_path.starts_with(&canonical_allowed) {
            return Err(SubstitutionError::SecurityError(format!(
                "File references outside the rustycode config directory are not allowed: {path}"
            )));
        }

        // Check cache
        if let Some(cached) = self.cache.get(path) {
            if let Some(ttl) = cached.ttl {
                if cached.timestamp.elapsed().unwrap_or_default() < ttl {
                    return Ok(cached.value.clone());
                }
            } else {
                return Ok(cached.value.clone());
            }
        }

        // Read file (using the canonical path)
        let content = std::fs::read_to_string(&canonical_path)
            .map_err(|e| SubstitutionError::FileReadError(canonical_path.clone(), e.to_string()))?;

        let trimmed = content.trim().to_string();

        // Cache result (5 minute TTL)
        self.cache.insert(
            path.to_string(),
            CachedValue {
                value: trimmed.clone(),
                timestamp: SystemTime::now(),
                ttl: Some(Duration::from_mins(5)),
            },
        );

        Ok(trimmed)
    }

    /// Expand shell-style environment variable references: `${VAR}` and `${VAR:-default}`.
    ///
    /// This matches Claude Code's MCP config syntax where values like
    /// `"Bearer ${API_KEY}"` or `"${DB_URL:-postgresql://localhost/mydb}"` are
    /// resolved at config load time.
    #[allow(clippy::unused_self)]
    pub fn expand_env_vars(&self, input: &str) -> String {
        let chars: Vec<char> = input.chars().collect();
        let len = chars.len();
        let mut result = String::with_capacity(input.len());
        let mut i = 0;

        while i < len {
            if i + 1 < len && chars[i] == '$' && chars[i + 1] == '{' {
                let start = i + 2;
                if let Some(relative_pos) = chars[start..].iter().position(|&c| c == '}') {
                    let end = start + relative_pos;
                    let var_part: String = chars[start..end].iter().collect();
                    let (var_name, default) =
                        var_part.find(":-").map_or((&var_part[..], None), |pos| {
                            (&var_part[..pos], Some(&var_part[pos + 2..]))
                        });

                    let value = std::env::var(var_name)
                        .ok()
                        .or_else(|| default.map(ToString::to_string))
                        .unwrap_or_default();

                    result.push_str(&value);
                    i = end + 1;
                    continue;
                }
            }
            result.push(chars[i]);
            i += 1;
        }

        result
    }

    #[allow(clippy::unused_self, clippy::unnecessary_wraps)]
    fn expand_tilde(&self, path: &str) -> Result<PathBuf, SubstitutionError> {
        if let Some(stripped) = path.strip_prefix("~/") {
            if let Some(home) = dirs::home_dir() {
                return Ok(home.join(stripped));
            }
        }

        Ok(PathBuf::from(path))
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SubstitutionError {
    #[error("Recursion limit exceeded")]
    RecursionLimitExceeded,

    #[error("Unterminated substitution")]
    UnterminatedSubstitution,

    #[error("Invalid substitution format: {0}")]
    InvalidFormat(String),

    #[error("Unknown substitution kind: {0}")]
    UnknownKind(String),

    #[error("Environment variable not found: {0}")]
    EnvVarNotFound(String),

    #[error("Failed to read file {0}: {1}")]
    FileReadError(PathBuf, String),

    #[error("Security error: {0}")]
    SecurityError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_substitution() {
        std::env::set_var("TEST_VAR", "test_value");

        let mut engine = SubstitutionEngine::new();
        let result = engine.process("{env:TEST_VAR}");

        assert_eq!(result.unwrap(), "test_value");
    }

    #[test]
    fn test_file_substitution() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "file content").unwrap();

        let mut engine = SubstitutionEngine::new().with_allowed_base(temp_dir.path().to_path_buf());
        let result = engine.process(&format!("{{file:{}}}", file_path.display()));

        assert_eq!(result.unwrap(), "file content");
    }

    #[test]
    fn test_nested_substitutions() {
        // Use a temporary file so tests don't depend on absolute paths
        let temp_dir = tempfile::tempdir().unwrap();
        let key_path = temp_dir.path().join("key.txt");
        std::fs::write(&key_path, "secret-key").unwrap();
        std::env::set_var("KEY_FILE", key_path.to_string_lossy().to_string());

        let mut engine = SubstitutionEngine::new().with_allowed_base(temp_dir.path().to_path_buf());
        let result = engine.process("{file:{env:KEY_FILE}}");

        assert_eq!(result.unwrap(), "secret-key");
    }

    #[test]
    fn test_default_substitution() {
        let mut engine = SubstitutionEngine::new();
        let result = engine.process("{default:default_value}");

        assert_eq!(result.unwrap(), "default_value");
    }

    #[test]
    fn test_expand_env_vars_simple() {
        std::env::set_var("MY_API_KEY", "secret123");
        let engine = SubstitutionEngine::new();
        let result = engine.expand_env_vars("Bearer ${MY_API_KEY}");
        assert_eq!(result, "Bearer secret123");
    }

    #[test]
    fn test_expand_env_vars_default_used() {
        std::env::remove_var("NONEXISTENT_VAR_XYZ");
        let engine = SubstitutionEngine::new();
        let result = engine.expand_env_vars("${NONEXISTENT_VAR_XYZ:-fallback}");
        assert_eq!(result, "fallback");
    }

    #[test]
    fn test_expand_env_vars_default_not_used_when_set() {
        std::env::set_var("EXISTING_VAR_ABC", "actual");
        let engine = SubstitutionEngine::new();
        let result = engine.expand_env_vars("${EXISTING_VAR_ABC:-fallback}");
        assert_eq!(result, "actual");
    }

    #[test]
    fn test_expand_env_vars_missing_no_default() {
        std::env::remove_var("TOTALLY_MISSING_VAR");
        let engine = SubstitutionEngine::new();
        let result = engine.expand_env_vars("key=${TOTALLY_MISSING_VAR}");
        assert_eq!(result, "key=");
    }

    #[test]
    fn test_expand_env_vars_multiple() {
        std::env::set_var("HOST", "localhost");
        std::env::set_var("PORT", "5432");
        let engine = SubstitutionEngine::new();
        let result = engine.expand_env_vars("postgresql://${HOST}:${PORT}/mydb");
        assert_eq!(result, "postgresql://localhost:5432/mydb");
    }

    #[test]
    fn test_expand_env_vars_no_vars() {
        let engine = SubstitutionEngine::new();
        let result = engine.expand_env_vars("plain string no vars");
        assert_eq!(result, "plain string no vars");
    }

    #[test]
    fn test_expand_env_vars_url_with_default() {
        std::env::remove_var("API_BASE_URL");
        let engine = SubstitutionEngine::new();
        let result = engine.expand_env_vars("${API_BASE_URL:-https://api.example.com}/mcp");
        assert_eq!(result, "https://api.example.com/mcp");
    }
}
