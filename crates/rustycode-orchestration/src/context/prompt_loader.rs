//! Prompt Loader
//!
//! Reads .md prompt templates from the prompts/ directory and substitutes
//! {{variable}} placeholders with provided values.
//!
//! Templates live at prompts/ relative to the project root.
//! They use {{variableName}} syntax for substitution.
//!
//! All templates are eagerly loaded into cache at module init via `warm_cache()`.
//! This prevents a running session from being invalidated when another launch
//! overwrites template files on disk with newer versions.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use crate::error::{OrchestrationError, Result};

// ─── Constants ───────────────────────────────────────────────────────────────

/// Prompts directory name
const PROMPTS_DIR: &str = "prompts";
/// Templates directory name
const TEMPLATES_DIR: &str = "templates";
/// Template cache key prefix
const TEMPLATE_PREFIX: &str = "tpl:";

// ─── State ───────────────────────────────────────────────────────────────────

/// Global template cache
struct TemplateCache {
    /// Cached template contents
    templates: HashMap<String, String>,
    /// Base directory for prompts/templates
    base_dir: Option<PathBuf>,
}

impl TemplateCache {
    fn new() -> Self {
        Self {
            templates: HashMap::new(),
            base_dir: None,
        }
    }
}

/// Global state
static CACHE: OnceLock<Mutex<TemplateCache>> = OnceLock::new();

fn cache() -> &'static Mutex<TemplateCache> {
    CACHE.get_or_init(|| Mutex::new(TemplateCache::new()))
}

// ─── Configuration ───────────────────────────────────────────────────────────

/// Set the base directory for prompts and templates
pub fn set_base_dir(dir: PathBuf) {
    let mut c = cache()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    c.base_dir = Some(dir);
}

/// Get the base directory
pub fn get_base_dir() -> Option<PathBuf> {
    let c = cache()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    c.base_dir.clone()
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Load a prompt template and substitute variables.
///
/// # Arguments
/// * `name` - Template filename without .md extension (e.g. "execute-task")
/// * `vars` - Key-value pairs to substitute for {{key}} placeholders
///
/// # Returns
/// The template content with variables substituted
///
/// # Errors
/// Returns `OrchestrationError::PromptLoad` if:
/// - Template file cannot be read
/// - Template declares variables not provided in vars
#[allow(clippy::implicit_hasher)]
#[allow(clippy::items_after_statements)]
pub fn load_prompt(name: &str, vars: &HashMap<String, String>) -> Result<String> {
    let mut c = cache()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    // Try cache first
    if !c.templates.contains_key(name) {
        // Load from disk
        let content = load_prompt_file(name, c.base_dir.as_ref())?;
        c.templates.insert(name.to_string(), content);
    }

    let content = c
        .templates
        .get(name)
        .ok_or_else(|| OrchestrationError::PromptLoad {
            template: name.to_string(),
            missing: vec![],
            hint: format!("Template '{name}' not found in cache"),
        })?;

    // Check BEFORE substitution: find all {{varName}} placeholders the template
    // declares and verify every one has a value in vars.
    let declared = extract_placeholders(content);
    let missing: Vec<&str> = declared
        .iter()
        .filter(|key| !vars.contains_key(*key))
        .map(String::as_str)
        .collect();

    if !missing.is_empty() {
        return Err(OrchestrationError::PromptLoad {
            template: name.to_string(),
            missing: missing.into_iter().map(String::from).collect(),
            hint: "This usually means the code in memory is older than the template on disk. Restart to reload.".to_string(),
        });
    }

    // Substitute variables
    let mut result = content.clone();
    for (key, value) in vars {
        result = result.replace(&format!("{{{{{key}}}}}"), value);
    }

    drop(c);

    Ok(result.trim().to_string())
}

/// Load a raw template file from the templates/ directory.
///
/// Templates are cached with a `tpl:` prefix to avoid collisions with prompt cache keys.
///
/// # Arguments
/// * `name` - Template filename without .md extension
///
/// # Returns
/// The template content
pub fn load_template(name: &str) -> Result<String> {
    let cache_key = format!("{TEMPLATE_PREFIX}{name}");
    let mut c = cache()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    if !c.templates.contains_key(&cache_key) {
        let content = load_template_file(name, c.base_dir.as_ref())?;
        c.templates.insert(cache_key.clone(), content);
    }

    Ok(c.templates
        .get(&cache_key)
        .ok_or_else(|| OrchestrationError::PromptLoad {
            template: name.to_string(),
            missing: vec![],
            hint: format!("Template '{name}' not found in cache after insert"),
        })?
        .trim()
        .to_string())
}

/// Load a template and wrap it with a labeled footer for inlining into prompts.
///
/// The template body is emitted first so that any YAML frontmatter (---) remains
/// at the first non-whitespace line of the template content.
///
/// # Arguments
/// * `name` - Template filename without .md extension
/// * `label` - Label to add in the footer
///
/// # Returns
/// The template content with labeled footer
pub fn inline_template(name: &str, label: &str) -> Result<String> {
    let content = load_template(name)?;
    Ok(format!(
        "{content}\n\n### Output Template: {label}\nSource: `templates/{name}.md`"
    ))
}

/// Clear the template cache
pub fn clear_cache() {
    let mut c = cache()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    c.templates.clear();
}

// ─── Internals ───────────────────────────────────────────────────────────────

/// Load a prompt file from disk
fn load_prompt_file(name: &str, base_dir: Option<&PathBuf>) -> Result<String> {
    let path = resolve_prompt_path(name, base_dir)?;
    fs::read_to_string(&path).map_err(|e| OrchestrationError::PromptLoad {
        template: name.to_string(),
        missing: vec![],
        hint: format!("Failed to read prompt file {}: {e}", path.display()),
    })
}

/// Load a template file from disk
fn load_template_file(name: &str, base_dir: Option<&PathBuf>) -> Result<String> {
    let path = resolve_template_path(name, base_dir)?;
    fs::read_to_string(&path).map_err(|e| OrchestrationError::PromptLoad {
        template: name.to_string(),
        missing: vec![],
        hint: format!("Failed to read template file {}: {e}", path.display()),
    })
}

/// Resolve the path to a prompt file
fn resolve_prompt_path(name: &str, base_dir: Option<&PathBuf>) -> Result<PathBuf> {
    let base = base_dir
        .as_ref()
        .ok_or_else(|| OrchestrationError::Configuration {
            message: "Prompt base directory not set".to_string(),
        })?;
    let path = base.join(PROMPTS_DIR).join(format!("{name}.md"));
    Ok(path)
}

/// Resolve the path to a template file
fn resolve_template_path(name: &str, base_dir: Option<&PathBuf>) -> Result<PathBuf> {
    let base = base_dir
        .as_ref()
        .ok_or_else(|| OrchestrationError::Configuration {
            message: "Template base directory not set".to_string(),
        })?;
    let path = base.join(TEMPLATES_DIR).join(format!("{name}.md"));
    Ok(path)
}

/// Extract all {{variableName}} placeholders from template content
fn extract_placeholders(content: &str) -> HashSet<String> {
    let mut placeholders = HashSet::new();
    let mut chars = content.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '{' && chars.peek() == Some(&'{') {
            chars.next(); // consume second '{'

            // Extract variable name
            let mut var_name = String::new();
            let mut has_content = false;

            while let Some(&next) = chars.peek() {
                chars.next();
                if next == '}' && chars.peek() == Some(&'}') {
                    chars.next(); // consume second '}'
                    if has_content && var_name.chars().next().is_some_and(char::is_alphabetic) {
                        placeholders.insert(var_name.clone());
                    }
                    break;
                }
                if next.is_alphanumeric() || next == '_' {
                    var_name.push(next);
                    has_content = true;
                } else {
                    break;
                }
            }
        }
    }

    placeholders
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};
    use tempfile::TempDir;

    fn test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn test_extract_placeholders() {
        let _guard = test_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let content = "Hello {{name}}, your {{status}} is active.";
        let placeholders = extract_placeholders(content);
        assert_eq!(placeholders.len(), 2);
        assert!(placeholders.contains("name"));
        assert!(placeholders.contains("status"));
    }

    #[test]
    fn test_extract_placeholders_empty() {
        let _guard = test_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let content = "No placeholders here.";
        let placeholders = extract_placeholders(content);
        assert!(placeholders.is_empty());
    }

    #[test]
    fn test_extract_placeholders_underscores() {
        let _guard = test_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let content = "{{user_name}} and {{max_count}}";
        let placeholders = extract_placeholders(content);
        assert_eq!(placeholders.len(), 2);
        assert!(placeholders.contains("user_name"));
        assert!(placeholders.contains("max_count"));
    }

    #[test]
    fn test_extract_placeholders_malformed() {
        let _guard = test_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let content = "{single} and {{double}}";
        let placeholders = extract_placeholders(content);
        assert_eq!(placeholders.len(), 1);
        assert!(placeholders.contains("double"));
    }

    #[test]
    fn test_extract_placeholders_must_start_with_letter() {
        let _guard = test_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let content = "{{valid}} and {{123invalid}}";
        let placeholders = extract_placeholders(content);
        assert_eq!(placeholders.len(), 1);
        assert!(placeholders.contains("valid"));
    }

    #[test]
    fn test_load_prompt_missing_vars() {
        let _guard = test_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let temp_dir = TempDir::new().unwrap();
        set_base_dir(temp_dir.path().to_path_buf());
        clear_cache();

        let prompts_dir = temp_dir.path().join("prompts");
        fs::create_dir_all(&prompts_dir).unwrap();

        let prompt_file = prompts_dir.join("test.md");
        fs::write(&prompt_file, "Hello {{name}}, your {{status}} is active.").unwrap();

        let vars = HashMap::from([("name".to_string(), "Alice".to_string())]);
        let result = load_prompt("test", &vars);

        assert!(result.is_err());
        if let Err(OrchestrationError::PromptLoad { missing, .. }) = result {
            assert!(missing.contains(&"status".to_string()));
        } else {
            panic!("Expected PromptLoad error");
        }
    }

    #[test]
    fn test_load_prompt_success() {
        let _guard = test_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let temp_dir = TempDir::new().unwrap();
        set_base_dir(temp_dir.path().to_path_buf());
        clear_cache();

        let prompts_dir = temp_dir.path().join("prompts");
        fs::create_dir_all(&prompts_dir).unwrap();

        let prompt_file = prompts_dir.join("greeting.md");
        fs::write(&prompt_file, "Hello {{name}}!").unwrap();

        let vars = HashMap::from([("name".to_string(), "Bob".to_string())]);
        let result = load_prompt("greeting", &vars).unwrap();

        assert_eq!(result, "Hello Bob!");
    }

    #[test]
    fn test_load_prompt_trimmed() {
        let _guard = test_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let temp_dir = TempDir::new().unwrap();
        set_base_dir(temp_dir.path().to_path_buf());
        clear_cache();

        let prompts_dir = temp_dir.path().join("prompts");
        fs::create_dir_all(&prompts_dir).unwrap();

        let prompt_file = prompts_dir.join("spaces.md");
        fs::write(&prompt_file, "  \n  Hello {{name}}!  \n  ").unwrap();

        let vars = HashMap::from([("name".to_string(), "Carol".to_string())]);
        let result = load_prompt("spaces", &vars).unwrap();

        assert_eq!(result, "Hello Carol!");
    }

    #[test]
    fn test_load_template() {
        let _guard = test_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let temp_dir = TempDir::new().unwrap();
        set_base_dir(temp_dir.path().to_path_buf());
        clear_cache();

        let templates_dir = temp_dir.path().join("templates");
        fs::create_dir_all(&templates_dir).unwrap();

        let template_file = templates_dir.join("output.md");
        fs::write(&template_file, "# Output\n\nContent here").unwrap();

        let result = load_template("output").unwrap();
        assert_eq!(result, "# Output\n\nContent here");
    }

    #[test]
    fn test_inline_template() {
        let _guard = test_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let temp_dir = TempDir::new().unwrap();
        set_base_dir(temp_dir.path().to_path_buf());
        clear_cache();

        let templates_dir = temp_dir.path().join("templates");
        fs::create_dir_all(&templates_dir).unwrap();

        let template_file = templates_dir.join("task.md");
        fs::write(&template_file, "# Task Summary\n\nCompleted").unwrap();

        let result = inline_template("task", "Task Output").unwrap();
        assert!(result.contains("# Task Summary"));
        assert!(result.contains("### Output Template: Task Output"));
        assert!(result.contains("Source: `templates/task.md`"));
    }

    #[test]
    fn test_load_prompt_caching() {
        let _guard = test_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let temp_dir = TempDir::new().unwrap();
        set_base_dir(temp_dir.path().to_path_buf());
        clear_cache();

        let prompts_dir = temp_dir.path().join("prompts");
        fs::create_dir_all(&prompts_dir).unwrap();

        let prompt_file = prompts_dir.join("cache.md");
        fs::write(&prompt_file, "Cached: {{value}}").unwrap();

        let vars = HashMap::from([("value".to_string(), "1".to_string())]);
        let result1 = load_prompt("cache", &vars).unwrap();
        assert_eq!(result1, "Cached: 1");

        // Modify file on disk
        fs::write(&prompt_file, "Modified: {{value}}").unwrap();

        // Should still return cached content
        let vars2 = HashMap::from([("value".to_string(), "2".to_string())]);
        let result2 = load_prompt("cache", &vars2).unwrap();
        assert_eq!(result2, "Cached: 2"); // Still using original template
    }

    #[test]
    fn test_clear_cache() {
        let _guard = test_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let temp_dir = TempDir::new().unwrap();
        set_base_dir(temp_dir.path().to_path_buf());
        clear_cache();

        let prompts_dir = temp_dir.path().join("prompts");
        fs::create_dir_all(&prompts_dir).unwrap();

        let prompt_file = prompts_dir.join("clear.md");
        fs::write(&prompt_file, "Before: {{value}}").unwrap();

        let vars = HashMap::from([("value".to_string(), "1".to_string())]);
        let result1 = load_prompt("clear", &vars).unwrap();
        assert_eq!(result1, "Before: 1");

        // Clear cache and modify file
        clear_cache();
        fs::write(&prompt_file, "After: {{value}}").unwrap();

        let vars2 = HashMap::from([("value".to_string(), "2".to_string())]);
        let result2 = load_prompt("clear", &vars2).unwrap();
        assert_eq!(result2, "After: 2"); // Now using modified template
    }

    #[test]
    fn test_multiple_substitutions() {
        let _guard = test_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let temp_dir = TempDir::new().unwrap();
        set_base_dir(temp_dir.path().to_path_buf());
        clear_cache();

        let prompts_dir = temp_dir.path().join("prompts");
        fs::create_dir_all(&prompts_dir).unwrap();

        let prompt_file = prompts_dir.join("multi.md");
        fs::write(&prompt_file, "{{a}} {{b}} {{a}}").unwrap();

        let vars = HashMap::from([
            ("a".to_string(), "X".to_string()),
            ("b".to_string(), "Y".to_string()),
        ]);
        let result = load_prompt("multi", &vars).unwrap();

        assert_eq!(result, "X Y X");
    }

    #[test]
    fn test_empty_template() {
        let _guard = test_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let temp_dir = TempDir::new().unwrap();
        set_base_dir(temp_dir.path().to_path_buf());
        clear_cache();

        let prompts_dir = temp_dir.path().join("prompts");
        fs::create_dir_all(&prompts_dir).unwrap();

        let prompt_file = prompts_dir.join("empty.md");
        fs::write(&prompt_file, "").unwrap();

        let vars = HashMap::new();
        let result = load_prompt("empty", &vars).unwrap();

        assert_eq!(result, "");
    }

    #[test]
    fn test_template_cache_key_prefix() {
        let _guard = test_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let temp_dir = TempDir::new().unwrap();
        set_base_dir(temp_dir.path().to_path_buf());
        clear_cache();

        let prompts_dir = temp_dir.path().join("prompts");
        fs::create_dir_all(&prompts_dir).unwrap();
        let templates_dir = temp_dir.path().join("templates");
        fs::create_dir_all(&templates_dir).unwrap();

        // Create both a prompt and a template with the same name
        fs::write(prompts_dir.join("test.md"), "Prompt: {{value}}").unwrap();
        fs::write(templates_dir.join("test.md"), "Template content").unwrap();

        // They should not collide
        let vars = HashMap::from([("value".to_string(), "A".to_string())]);
        let prompt_result = load_prompt("test", &vars).unwrap();
        let template_result = load_template("test").unwrap();

        assert_eq!(prompt_result, "Prompt: A");
        assert_eq!(template_result, "Template content");
    }
}
