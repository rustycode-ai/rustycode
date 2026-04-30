#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::uninlined_format_args
    )
)]
use crate::{JsoncParser, SubstitutionEngine};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Returns true if the JSON value is present and not Null.
const fn is_present(v: Option<&serde_json::Value>) -> bool {
    !matches!(v, None | Some(serde_json::Value::Null))
}

/// Merge auto-detected project tools into the config if not explicitly set.
fn apply_auto_detection(mut merged: serde_json::Value, project_dir: &Path) -> serde_json::Value {
    let has_project_tools = is_present(merged.get("advanced").and_then(|a| a.get("project_tools")));
    let has_lsp_config = is_present(merged.get("advanced").and_then(|a| a.get("lsp_config")));

    if has_project_tools || has_lsp_config {
        return merged;
    }

    // Auto-detect project type
    if let Some(detection) = rustycode_lsp::ProjectDetector::detect(project_dir) {
        let project_tools = serde_json::json!({
            "build_system": detection.build_system.to_string(),
            "linters": detection.linters,
            "formatters": detection.formatters,
            "lsp_config": detection.lsp_config
        });

        // Ensure advanced section exists
        if merged.get("advanced").is_none() {
            merged
                .as_object_mut()
                .map(|m| m.insert("advanced".into(), serde_json::json!({})));
        }

        // Add project_tools and lsp_config to advanced
        let lsp_config_value =
            if let Some(serde_json::Value::Object(pt)) = project_tools.get("lsp_config") {
                Some(serde_json::Value::Object(pt.clone()))
            } else {
                None
            };

        if let Some(serde_json::Value::Object(advanced)) = merged.get_mut("advanced") {
            advanced.insert("project_tools".into(), project_tools);
            // Also set lsp_config at the top level for easier access
            if let Some(lsp_cfg) = lsp_config_value {
                advanced.insert("lsp_config".into(), lsp_cfg);
            }
        }
    }

    merged
}

/// Cached configuration data with modification time
#[derive(Clone)]
struct CachedConfig {
    data: serde_json::Value,
    modified_time: std::time::SystemTime,
}

/// Configuration loader with hierarchical merging, substitution support, and caching
pub struct ConfigLoader {
    jsonc_parser: JsoncParser,
    substitution_engine: SubstitutionEngine,
    cache: Arc<RwLock<HashMap<PathBuf, CachedConfig>>>,
}

/// Apply substitutions only to string values inside the parsed JSON.
/// This avoids interpreting JSON structural braces as substitution tokens.
fn apply_subs_to_value(
    engine: &mut SubstitutionEngine,
    val: serde_json::Value,
) -> Result<serde_json::Value, String> {
    match val {
        serde_json::Value::String(s) => engine
            .process(&s)
            .map(serde_json::Value::String)
            .map_err(|e| format!("Substitution error: {e}")),
        serde_json::Value::Array(arr) => {
            let mut out = Vec::with_capacity(arr.len());
            for item in arr {
                out.push(apply_subs_to_value(engine, item)?);
            }
            Ok(serde_json::Value::Array(out))
        }
        serde_json::Value::Object(map) => {
            let mut out_map = serde_json::Map::with_capacity(map.len());
            for (k, v) in map {
                out_map.insert(k, apply_subs_to_value(engine, v)?);
            }
            Ok(serde_json::Value::Object(out_map))
        }
        other => Ok(other),
    }
}

/// Deep merge two JSON values with override strategy
fn deep_merge(base: serde_json::Value, override_: serde_json::Value) -> serde_json::Value {
    match (base, override_) {
        (serde_json::Value::Object(mut base_map), serde_json::Value::Object(override_map)) => {
            for (key, override_value) in override_map {
                let base_value = base_map.remove(&key);

                let merged = match (base_value, override_value) {
                    (Some(base_val), serde_json::Value::Object(override_obj)) => {
                        if let serde_json::Value::Object(base_obj) = base_val {
                            deep_merge(
                                serde_json::Value::Object(base_obj),
                                serde_json::Value::Object(override_obj),
                            )
                        } else {
                            serde_json::Value::Object(override_obj)
                        }
                    }
                    (_, override_val) => override_val,
                };

                base_map.insert(key, merged);
            }
            serde_json::Value::Object(base_map)
        }
        (_, override_val) => override_val,
    }
}

impl ConfigLoader {
    pub fn new() -> Self {
        Self {
            jsonc_parser: JsoncParser::new(),
            substitution_engine: SubstitutionEngine::new(),
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Load configuration from a project directory with hierarchical merging
    pub fn load(&mut self, project_dir: &Path) -> Result<serde_json::Value, String> {
        // Start with the library defaults so missing required fields (like `model`) are present
        let mut merged = serde_json::to_value(crate::Config::default())
            .unwrap_or_else(|_| serde_json::json!({}));

        // Load configs in priority order (lowest to highest)
        // Try each path directly - NotFound errors are silently ignored (file may not exist)
        // This avoids TOCTOU: checking exists() then reading creates a race window
        for config_path in self.search_paths(project_dir) {
            match self.load_from_path(&config_path) {
                Ok(config_value) => {
                    merged = deep_merge(merged, config_value);
                }
                Err(e) => {
                    // Silently skip "file not found" (expected for optional configs),
                    // but log other errors (parse errors, permission denied, etc.)
                    let err_str = e.to_lowercase();
                    if !err_str.contains("no such file")
                        && !err_str.contains("not found")
                        && !err_str.contains("does not exist")
                    {
                        tracing::debug!(
                            "Config file {} skipped due to error: {}",
                            config_path.display(),
                            e
                        );
                    }
                }
            }
        }

        // Apply auto-detection for project tools if not explicitly configured
        merged = apply_auto_detection(merged, project_dir);

        Ok(merged)
    }

    /// Load configuration from a specific path with caching
    pub async fn load_from_path_async(
        &mut self,
        path: &PathBuf,
    ) -> Result<serde_json::Value, String> {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.get(path) {
                // Verify file hasn't been modified
                if let Ok(metadata) = tokio::fs::metadata(path).await {
                    if let Ok(modified) = metadata.modified() {
                        if modified == cached.modified_time {
                            return Ok(cached.data.clone());
                        }
                    }
                }
            }
        }

        // Cache miss or stale - load from disk
        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| format!("Failed to read config file {}: {}", path.display(), e))?;

        // Parse as JSON/JSONC (TOML support removed)
        let parsed = self
            .jsonc_parser
            .parse_str(&content)
            .map_err(|e| format!("Failed to parse JSONC: {e}"))?;

        let final_value = apply_subs_to_value(&mut self.substitution_engine, parsed)
            .map_err(|e| format!("Failed to apply substitutions: {e}"))?;

        // Update cache
        if let Ok(metadata) = tokio::fs::metadata(path).await {
            if let Ok(modified) = metadata.modified() {
                let cached = CachedConfig {
                    data: final_value.clone(),
                    modified_time: modified,
                };
                let mut cache = self.cache.write().await;
                cache.insert(path.clone(), cached);
            }
        }

        Ok(final_value)
    }

    /// Load configuration from a specific path (synchronous version for compatibility)
    pub fn load_from_path(&mut self, path: &PathBuf) -> Result<serde_json::Value, String> {
        // Read file
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read config file {}: {}", path.display(), e))?;

        // Parse as JSON/JSONC (TOML support removed)
        let parsed = self
            .jsonc_parser
            .parse_str(&content)
            .map_err(|e| format!("Failed to parse JSONC: {e}"))?;

        let final_value = apply_subs_to_value(&mut self.substitution_engine, parsed)
            .map_err(|e| format!("Failed to apply substitutions: {e}"))?;

        Ok(final_value)
    }

    /// Get search paths for configuration files in priority order
    #[allow(clippy::unused_self)]
    fn search_paths(&self, project_dir: &Path) -> Vec<PathBuf> {
        let mut paths = Vec::new();

        // Legacy global config (for backwards compatibility)
        // This should be checked before XDG to maintain compatibility with existing installations
        if let Ok(home) = std::env::var("HOME") {
            let legacy_dir = PathBuf::from(home).join(".rustycode");
            paths.push(legacy_dir.join("config.json"));
            paths.push(legacy_dir.join("config.jsonc"));
        }

        // Global config (use XDG config dir)
        if let Some(cfg) = dirs::config_dir() {
            paths.push(cfg.join("rustycode").join("config.json"));
            paths.push(cfg.join("rustycode").join("config.jsonc"));
        }

        // Workspace config (search upward for .rustycode-workspace)
        let mut current = project_dir.to_path_buf();
        let mut depth = 0;
        while let Some(parent) = current.parent() {
            if depth >= 20 {
                break;
            }
            paths.push(parent.join(".rustycode-workspace").join("config.json"));
            paths.push(parent.join(".rustycode-workspace").join("config.jsonc"));
            current = parent.to_path_buf();
            depth += 1;
        }

        // Project config: directory-based configs (JSON/JSONC only, TOML support removed)
        paths.push(project_dir.join(".rustycode").join("config.json"));
        paths.push(project_dir.join(".rustycode").join("config.jsonc"));

        paths
    }

    /// Clear the configuration cache
    pub async fn clear_cache(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }

    /// Get cache size (number of cached configs)
    pub async fn cache_size(&self) -> usize {
        let cache = self.cache.read().await;
        cache.len()
    }
}

impl Default for ConfigLoader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn test_deep_merge_flat_objects() {
        let base = serde_json::json!({"a": 1, "b": 2});
        let override_ = serde_json::json!({"b": 3, "c": 4});
        let merged = deep_merge(base, override_);
        assert_eq!(merged["a"], 1);
        assert_eq!(merged["b"], 3);
        assert_eq!(merged["c"], 4);
    }

    #[test]
    fn test_deep_merge_nested_objects() {
        let base = serde_json::json!({"model": {"name": "sonnet", "temperature": 0.7}});
        let override_ = serde_json::json!({"model": {"temperature": 0.9}});
        let merged = deep_merge(base, override_);
        assert_eq!(merged["model"]["name"], "sonnet");
        assert_eq!(merged["model"]["temperature"], 0.9);
    }

    #[test]
    fn test_deep_merge_override_replaces_non_object() {
        let base = serde_json::json!({"key": "string"});
        let override_ = serde_json::json!({"key": {"nested": true}});
        let merged = deep_merge(base, override_);
        assert_eq!(merged["key"]["nested"], true);
    }

    #[test]
    fn test_deep_merge_override_non_object_with_scalar() {
        let base = serde_json::json!({"key": {"nested": true}});
        let override_ = serde_json::json!({"key": "scalar"});
        let merged = deep_merge(base, override_);
        assert_eq!(merged["key"], "scalar");
    }

    #[test]
    fn test_deep_merge_non_object_base_returns_override() {
        let base = serde_json::json!("not an object");
        let override_ = serde_json::json!({"a": 1});
        let merged = deep_merge(base, override_);
        assert_eq!(merged["a"], 1);
    }

    #[test]
    fn test_deep_merge_empty_base() {
        let base = serde_json::json!({});
        let override_ = serde_json::json!({"a": 1, "b": 2});
        let merged = deep_merge(base, override_);
        assert_eq!(merged["a"], 1);
        assert_eq!(merged["b"], 2);
    }

    #[test]
    fn test_deep_merge_empty_override() {
        let base = serde_json::json!({"a": 1});
        let override_ = serde_json::json!({});
        let merged = deep_merge(base, override_);
        assert_eq!(merged["a"], 1);
    }

    #[test]
    fn test_load_from_path_reads_json() {
        let dir = temp_dir();
        let config_path = dir.path().join("config.json");
        fs::write(&config_path, r#"{"model": "claude-3", "temperature": 0.5}"#).unwrap();

        let mut loader = ConfigLoader::new();
        let result = loader.load_from_path(&config_path).unwrap();
        assert_eq!(result["model"], "claude-3");
        assert_eq!(result["temperature"], 0.5);
    }

    #[test]
    fn test_load_from_path_reads_jsonc() {
        let dir = temp_dir();
        let config_path = dir.path().join("config.jsonc");
        fs::write(
            &config_path,
            r#"{ "model": "gpt-4", /* comment */ "verbose": true }"#,
        )
        .unwrap();

        let mut loader = ConfigLoader::new();
        let result = loader.load_from_path(&config_path).unwrap();
        assert_eq!(result["model"], "gpt-4");
        assert_eq!(result["verbose"], true);
    }

    #[test]
    fn test_load_from_path_nonexistent_file() {
        let mut loader = ConfigLoader::new();
        let result = loader.load_from_path(&PathBuf::from("/nonexistent/config.json"));
        assert!(result.is_err());
    }

    #[test]
    fn test_load_from_path_invalid_json() {
        let dir = temp_dir();
        let config_path = dir.path().join("bad.json");
        fs::write(&config_path, "not valid json {{{").unwrap();

        let mut loader = ConfigLoader::new();
        let result = loader.load_from_path(&config_path);
        assert!(result.is_err());
    }

    #[test]
    fn test_load_from_path_preserves_non_string_values() {
        let dir = temp_dir();
        let config_path = dir.path().join("config.json");
        fs::write(
            &config_path,
            r#"{"model": "test", "num": 42, "flag": true, "arr": [1,2]}"#,
        )
        .unwrap();

        let mut loader = ConfigLoader::new();
        let result = loader.load_from_path(&config_path).unwrap();
        assert_eq!(result["model"], "test");
        assert_eq!(result["num"], 42);
        assert_eq!(result["flag"], true);
        assert_eq!(result["arr"][0], 1);
    }

    #[test]
    fn test_search_paths_includes_project_config() {
        let dir = temp_dir();
        let loader = ConfigLoader::new();
        let paths = loader.search_paths(dir.path());

        let project_json = dir.path().join(".rustycode").join("config.json");
        assert!(paths.contains(&project_json));
    }

    #[test]
    fn test_search_paths_depth_capped_at_20() {
        let dir = temp_dir();
        let loader = ConfigLoader::new();
        let paths = loader.search_paths(dir.path());
        // Count workspace config paths (each depth adds 2: .json and .jsonc)
        let workspace_paths: Vec<_> = paths
            .iter()
            .filter(|p| {
                p.to_str()
                    .is_some_and(|s| s.contains(".rustycode-workspace"))
            })
            .collect();
        // Should have at most 20 * 2 = 40 workspace config paths
        assert!(
            workspace_paths.len() <= 40,
            "Expected at most 40 workspace paths, got {}",
            workspace_paths.len()
        );
    }

    #[test]
    fn test_cache_size_starts_empty() {
        let loader = ConfigLoader::new();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let size = rt.block_on(loader.cache_size());
        assert_eq!(size, 0);
    }

    #[test]
    fn test_clear_cache() {
        let loader = ConfigLoader::new();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(loader.clear_cache());
        let size = rt.block_on(loader.cache_size());
        assert_eq!(size, 0);
    }

    // --- apply_auto_detection integration tests ---

    #[test]
    fn test_apply_auto_detection_rust_project() {
        let dir = temp_dir();
        // Create a Cargo.toml to signal a Rust project
        fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();

        let mut loader = ConfigLoader::new();
        let result = loader.load(dir.path()).unwrap();

        // Should have detected Rust project tools
        let project_tools = result.get("advanced").and_then(|a| a.get("project_tools"));
        assert!(
            project_tools.is_some(),
            "expected project_tools to be auto-detected"
        );

        let pt = project_tools.unwrap();
        let build_system = pt.get("build_system").and_then(|v| v.as_str()).unwrap();
        assert_eq!(build_system, "Cargo");

        // Should have linters and formatters
        let linters = pt.get("linters").and_then(|v| v.as_array());
        assert!(linters.is_some());
        let formatters = pt.get("formatters").and_then(|v| v.as_array());
        assert!(formatters.is_some());

        // Should have lsp_config with rust-analyzer
        let lsp_config = pt.get("lsp_config");
        assert!(lsp_config.is_some());
        let servers = lsp_config.unwrap().get("servers");
        assert!(servers.is_some());
    }

    #[test]
    fn test_apply_auto_detection_npm_project() {
        let dir = temp_dir();
        // Create a package.json to signal an npm project
        fs::write(dir.path().join("package.json"), "{}").unwrap();

        let mut loader = ConfigLoader::new();
        let result = loader.load(dir.path()).unwrap();

        let project_tools = result.get("advanced").and_then(|a| a.get("project_tools"));
        assert!(project_tools.is_some());

        let pt = project_tools.unwrap();
        let build_system = pt.get("build_system").and_then(|v| v.as_str()).unwrap();
        assert_eq!(build_system, "Npm");
    }

    #[test]
    fn test_apply_auto_detection_no_project_marker() {
        let dir = temp_dir();
        // Empty directory -- no project markers

        let mut loader = ConfigLoader::new();
        let result = loader.load(dir.path()).unwrap();

        // No project_tools should be added (serde converts Option::None -> Null, so use is_present)
        let project_tools = result.get("advanced").and_then(|a| a.get("project_tools"));
        assert!(
            !is_present(project_tools),
            "expected no auto-detection for empty dir, got: {:?}",
            project_tools
        );
    }

    #[test]
    fn test_apply_auto_detection_skipped_when_explicit_config_exists() {
        let dir = temp_dir();
        // Create a Rust project marker
        fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();
        // But also create explicit config with project_tools
        let config_dir = dir.path().join(".rustycode");
        fs::create_dir(&config_dir).unwrap();
        fs::write(
            config_dir.join("config.json"),
            r#"{"advanced": {"project_tools": {"build_system": "Maven", "linters": ["custom-linter"]}}}"#,
        )
        .unwrap();

        let mut loader = ConfigLoader::new();
        let result = loader.load(dir.path()).unwrap();

        // Explicit config should win -- should be Maven, not Cargo
        let pt = result
            .get("advanced")
            .and_then(|a| a.get("project_tools"))
            .and_then(|v| v.get("build_system"))
            .and_then(|v| v.as_str());
        assert_eq!(
            pt,
            Some("Maven"),
            "explicit config should override auto-detection"
        );

        let linters = result
            .get("advanced")
            .and_then(|a| a.get("project_tools"))
            .and_then(|v| v.get("linters"))
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>());
        let linters_vec = linters.as_deref();
        assert!(linters_vec.is_some());
        assert_eq!(linters_vec.unwrap(), &["custom-linter"]);
    }

    #[test]
    fn test_apply_auto_detection_explicit_lsp_config_skips_detection() {
        let dir = temp_dir();
        // Create a Rust project marker
        fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();
        // But also create explicit lsp_config (without project_tools)
        let config_dir = dir.path().join(".rustycode");
        fs::create_dir(&config_dir).unwrap();
        fs::write(
            config_dir.join("config.json"),
            r#"{"advanced": {"project_tools": null, "lsp_config": {"servers": {"rust-analyzer": {"command": "custom-rust-analyzer"}}}}}"#,
        )
        .unwrap();

        let mut loader = ConfigLoader::new();
        let result = loader.load(dir.path()).unwrap();

        // Explicit lsp_config should prevent auto-detection
        let project_tools = result.get("advanced").and_then(|a| a.get("project_tools"));
        assert!(
            !is_present(project_tools),
            "explicit lsp_config should skip auto-detection"
        );

        // But the explicit lsp_config should be present
        let lsp_config = result.get("advanced").and_then(|a| a.get("lsp_config"));
        assert!(lsp_config.is_some());
        let cmd = lsp_config
            .unwrap()
            .get("servers")
            .and_then(|v| v.get("rust-analyzer"))
            .and_then(|v| v.get("command"))
            .and_then(|v| v.as_str());
        assert_eq!(cmd, Some("custom-rust-analyzer"));
    }
}
