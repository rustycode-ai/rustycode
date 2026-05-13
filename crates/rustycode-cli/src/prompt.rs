//! # Interactive Prompt System
//!
//! This module provides a comprehensive, trait-based prompt system for interactive
//! user input in CLI applications. It supports confirmation prompts, selection prompts,
//! text input, and multi-select prompts with validation and default values.
//!
//! ## Features
//!
//! - **Trait-based design** - Easy to extend with custom prompt types
//! - **Validation support** - Validate user input with custom validators
//! - **Default values** - Provide sensible defaults for prompts
//! - **Non-interactive mode** - Support `--yes` flag to skip all prompts
//! - **Rich formatting** - Clear, user-friendly prompt display
//! - **Error handling** - Comprehensive error messages and recovery
//!
//! ## Usage
//!
//! ```ignore
//! use rustycode_cli::prompt::{Confirm, Select, Input, Prompt};
//!
//! // Simple confirmation
//! let confirmed = Confirm::new("Continue?")
//!     .with_default(true)
//!     .prompt()?;
//!
//! // Selection from choices
//! let choice = Select::new("Choose an option")
//!     .option("Option 1", "value1")
//!     .option("Option 2", "value2")
//!     .prompt()?;
//!
//! // Text input with validation
//! let name = Input::new("Enter your name")
//!     .with_default("Guest")
//!     .validate(|s| {
//!         if s.is_empty() {
//!             Err("Name cannot be empty".into())
//!         } else if s.len() < 3 {
//!             Err("Name must be at least 3 characters".into())
//!         } else {
//!             Ok(())
//!         }
//!     })
//!     .prompt()?;
//! ```
//!
//! ## Non-Interactive Mode
//!
//! When `PromptConfig::global_yes_enabled()` is true (typically via a `--yes` flag),
//! all prompts will use their default values without requiring user input:
//!
//! ```ignore
//! use rustycode_cli::prompt::{PromptConfig, Confirm, Prompt};
//!
//! // Enable global yes mode (typically from CLI flag)
//! PromptConfig::set_global_yes(true);
//!
//! // This will return the default value without prompting
//! let confirmed = Confirm::new("Continue?")
//!     .with_default(true)
//!     .prompt()?;
//! ```

use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Global configuration for prompts
#[derive(Debug, Clone)]
pub struct PromptConfig {
    /// When true, all prompts use defaults without user input
    global_yes: Arc<AtomicBool>,
}

impl Default for PromptConfig {
    fn default() -> Self {
        Self {
            global_yes: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl PromptConfig {
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable or disable global yes mode (non-interactive mode)
    ///
    /// When enabled, all prompts will use their default values without
    /// requiring user input. This is typically controlled by a `--yes` flag.
    pub fn set_global_yes(&self, enabled: bool) {
        self.global_yes.store(enabled, Ordering::SeqCst);
    }

    /// Check if global yes mode is enabled
    pub fn global_yes_enabled(&self) -> bool {
        self.global_yes.load(Ordering::SeqCst)
    }

    /// Get the global prompt config instance
    pub fn global() -> &'static Self {
        static CONFIG: std::sync::OnceLock<PromptConfig> = std::sync::OnceLock::new();
        CONFIG.get_or_init(PromptConfig::new)
    }

    /// Set global yes mode on the global config
    pub fn set_global_yes_enabled(enabled: bool) {
        Self::global().set_global_yes(enabled);
    }
}

/// Base trait for all prompt types
///
/// This trait defines the common interface for all prompt implementations.
/// It uses generic associated types (GAT) to allow each prompt to define
/// its output type while providing a uniform interface.
pub trait Prompt: Sized {
    /// The type of value this prompt produces
    type Output;

    /// Execute the prompt and return the user's response
    ///
    /// This method will:
    /// 1. Check if global yes mode is enabled and return the default
    /// 2. Display the prompt to the user
    /// 3. Read and validate user input
    /// 4. Return the validated value
    fn prompt(self) -> io::Result<Self::Output>;

    /// Execute the prompt with a custom config
    fn prompt_with_config(self, config: &PromptConfig) -> io::Result<Self::Output>;
}

/// Validation error type
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub message: String,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ValidationError {}

impl From<String> for ValidationError {
    fn from(s: String) -> Self {
        Self { message: s }
    }
}

impl From<&str> for ValidationError {
    fn from(s: &str) -> Self {
        Self {
            message: s.to_string(),
        }
    }
}

/// Validator function type
pub type Validator<T> = dyn Fn(&T) -> Result<(), ValidationError> + Send + Sync;

/// Confirmation prompt for yes/no questions
///
/// # Example
///
/// ```no_run
/// use rustycode_cli::prompt::{Confirm, Prompt};
///
/// let confirmed = Confirm::new("Do you want to continue?")
///     .prompt()
///     .unwrap();
/// ```
#[derive(Debug, Clone)]
pub struct Confirm {
    pub message: String,
    default: bool,
    config: Option<PromptConfig>,
}

impl Confirm {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            default: false,
            config: None,
        }
    }

    /// Set the default value
    pub fn with_default(mut self, default: bool) -> Self {
        self.default = default;
        self
    }

    /// Use a custom prompt config
    pub fn with_config(mut self, config: PromptConfig) -> Self {
        self.config = Some(config);
        self
    }

    fn render_prompt(&self) -> String {
        let default_hint = if self.default { "Y/n" } else { "y/N" };
        format!("{} [{}]: ", self.message, default_hint)
    }

    fn parse_input(&self, input: &str) -> Option<bool> {
        match input.trim().to_lowercase().as_str() {
            "y" | "yes" => Some(true),
            "n" | "no" => Some(false),
            "" => Some(self.default),
            _ => None,
        }
    }
}

impl Prompt for Confirm {
    type Output = bool;

    fn prompt(self) -> io::Result<Self::Output> {
        let config = self
            .config
            .clone()
            .unwrap_or_else(|| PromptConfig::global().clone());
        self.prompt_with_config(&config)
    }

    fn prompt_with_config(self, config: &PromptConfig) -> io::Result<Self::Output> {
        // If global yes is enabled, return default without prompting
        if config.global_yes_enabled() {
            return Ok(self.default);
        }

        let prompt_text = self.render_prompt();
        let mut stdout = io::stdout().lock();
        write!(stdout, "{}", prompt_text)?;
        stdout.flush()?;

        loop {
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;

            if let Some(result) = self.parse_input(&input) {
                return Ok(result);
            }

            writeln!(io::stderr(), "Invalid input. Please enter 'y' or 'n'.")?;
            write!(stdout, "{}", prompt_text)?;
            stdout.flush()?;
        }
    }
}

/// Selection prompt for choosing from a list of options
///
/// # Example
///
/// ```no_run
/// use rustycode_cli::prompt::{Select, Prompt};
///
/// let choice = Select::new("Choose a color")
///     .option("Red", "red")
///     .option("Green", "green")
///     .option("Blue", "blue")
///     .prompt()
///     .unwrap();
/// ```
#[derive(Clone)]
pub struct Select<T: Clone> {
    pub message: String,
    options: Vec<(String, T)>,
    default_index: Option<usize>,
    validator: Option<Arc<Validator<T>>>,
    config: Option<PromptConfig>,
}

impl<T: Clone> Select<T> {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            options: Vec::new(),
            default_index: None,
            validator: None,
            config: None,
        }
    }

    /// Add an option to the selection
    pub fn option(mut self, label: impl Into<String>, value: T) -> Self {
        self.options.push((label.into(), value));
        self
    }

    /// Add multiple options at once
    pub fn options(mut self, options: impl IntoIterator<Item = (impl Into<String>, T)>) -> Self {
        for (label, value) in options {
            self.options.push((label.into(), value));
        }
        self
    }

    /// Set the default option by index
    pub fn with_default(mut self, index: usize) -> Self {
        self.default_index = Some(index);
        self
    }

    /// Add a validator for the selected value
    pub fn validate(
        mut self,
        validator: impl Fn(&T) -> Result<(), ValidationError> + Send + Sync + 'static,
    ) -> Self {
        self.validator = Some(Arc::new(validator));
        self
    }

    /// Use a custom prompt config
    pub fn with_config(mut self, config: PromptConfig) -> Self {
        self.config = Some(config);
        self
    }

    fn render_options(&self) -> String {
        let mut output = String::new();
        output.push_str(&self.message);
        output.push('\n');

        for (i, (label, _)) in self.options.iter().enumerate() {
            let default_mark = self.default_index == Some(i);
            let marker = if default_mark { ">" } else { " " };
            output.push_str(&format!("  {} {}. {}\n", marker, i + 1, label));
        }

        output
    }

    fn parse_input(&self, input: &str) -> Option<usize> {
        match input.trim().parse::<usize>() {
            Ok(i) if i >= 1 && i <= self.options.len() => Some(i - 1),
            _ => None,
        }
    }
}

impl<T: Clone> Prompt for Select<T> {
    type Output = T;

    fn prompt(self) -> io::Result<Self::Output> {
        let config = self
            .config
            .clone()
            .unwrap_or_else(|| PromptConfig::global().clone());
        self.prompt_with_config(&config)
    }

    fn prompt_with_config(self, config: &PromptConfig) -> io::Result<Self::Output> {
        if self.options.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot prompt with empty options list",
            ));
        }

        if config.global_yes_enabled() {
            let index = self.default_index.unwrap_or(0);
            let selected = self.options.get(index).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "Default index {} out of range ({} options)",
                        index,
                        self.options.len()
                    ),
                )
            })?;
            return Ok(selected.1.clone());
        }

        let prompt_text = self.render_options();
        let mut stdout = io::stdout().lock();
        write!(stdout, "{}", prompt_text)?;
        write!(stdout, "Enter choice [1-{}]: ", self.options.len())?;
        stdout.flush()?;

        loop {
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;

            if let Some(index) = self.parse_input(&input) {
                let value = self.options[index].1.clone();

                // Run validator if present
                if let Some(validator) = &self.validator {
                    match validator(&value) {
                        Ok(_) => return Ok(value),
                        Err(err) => {
                            writeln!(io::stderr(), "Validation error: {}", err)?;
                            write!(stdout, "Enter choice [1-{}]: ", self.options.len())?;
                            stdout.flush()?;
                            continue;
                        }
                    }
                }

                return Ok(value);
            }

            writeln!(
                io::stderr(),
                "Invalid choice. Please enter a number between 1 and {}.",
                self.options.len()
            )?;
            write!(stdout, "Enter choice [1-{}]: ", self.options.len())?;
            stdout.flush()?;
        }
    }
}

/// Text input prompt
///
/// # Example
///
/// ```no_run
/// use rustycode_cli::prompt::{Input, Prompt};
///
/// let name = Input::new("Enter your name")
///     .with_default("Guest")
///     .prompt()
///     .unwrap();
/// ```
#[derive(Clone)]
pub struct Input {
    pub message: String,
    default: Option<String>,
    validator: Option<Arc<Validator<String>>>,
    config: Option<PromptConfig>,
}

impl Input {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            default: None,
            validator: None,
            config: None,
        }
    }

    /// Set the default value
    pub fn with_default(mut self, default: impl Into<String>) -> Self {
        self.default = Some(default.into());
        self
    }

    /// Add a validator for the input
    pub fn validate(
        mut self,
        validator: impl Fn(&String) -> Result<(), ValidationError> + Send + Sync + 'static,
    ) -> Self {
        self.validator = Some(Arc::new(validator));
        self
    }

    /// Use a custom prompt config
    pub fn with_config(mut self, config: PromptConfig) -> Self {
        self.config = Some(config);
        self
    }

    fn render_prompt(&self) -> String {
        if let Some(default) = &self.default {
            format!("{} [{}]: ", self.message, default)
        } else {
            format!("{}: ", self.message)
        }
    }
}

impl Prompt for Input {
    type Output = String;

    fn prompt(self) -> io::Result<Self::Output> {
        let config = self
            .config
            .clone()
            .unwrap_or_else(|| PromptConfig::global().clone());
        self.prompt_with_config(&config)
    }

    fn prompt_with_config(self, config: &PromptConfig) -> io::Result<Self::Output> {
        // If global yes is enabled, return default without prompting
        if config.global_yes_enabled() {
            return Ok(self.default.unwrap_or_default());
        }

        let prompt_text = self.render_prompt();
        let mut stdout = io::stdout().lock();
        write!(stdout, "{}", prompt_text)?;
        stdout.flush()?;

        loop {
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            let input = input.trim().to_string();

            // Use default if input is empty
            let value = if input.is_empty() {
                if let Some(default) = &self.default {
                    default.clone()
                } else {
                    input
                }
            } else {
                input
            };

            // Run validator if present
            if let Some(validator) = &self.validator {
                match validator(&value) {
                    Ok(_) => return Ok(value),
                    Err(err) => {
                        writeln!(io::stderr(), "Validation error: {}", err)?;
                        write!(stdout, "{}", prompt_text)?;
                        stdout.flush()?;
                        continue;
                    }
                }
            }

            return Ok(value);
        }
    }
}

/// Multi-select prompt for choosing multiple options
///
/// # Example
///
/// ```no_run
/// use rustycode_cli::prompt::{MultiSelect, Prompt};
///
/// let choices = MultiSelect::new("Select features to install")
///     .option("Feature 1", "feat1")
///     .option("Feature 2", "feat2")
///     .option("Feature 3", "feat3")
///     .prompt()
///     .unwrap();
/// ```
#[derive(Debug, Clone)]
pub struct MultiSelect<T: Clone> {
    pub message: String,
    options: Vec<(String, T)>,
    default_indices: Vec<usize>,
    min_selections: usize,
    max_selections: Option<usize>,
    config: Option<PromptConfig>,
}

impl<T: Clone> MultiSelect<T> {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            options: Vec::new(),
            default_indices: Vec::new(),
            min_selections: 0,
            max_selections: None,
            config: None,
        }
    }

    /// Add an option to the selection
    pub fn option(mut self, label: impl Into<String>, value: T) -> Self {
        self.options.push((label.into(), value));
        self
    }

    /// Add multiple options at once
    pub fn options(mut self, options: impl IntoIterator<Item = (impl Into<String>, T)>) -> Self {
        for (label, value) in options {
            self.options.push((label.into(), value));
        }
        self
    }

    /// Set default selected indices
    pub fn with_defaults(mut self, indices: Vec<usize>) -> Self {
        self.default_indices = indices;
        self
    }

    /// Set minimum number of selections required
    pub fn min_selections(mut self, min: usize) -> Self {
        self.min_selections = min;
        self
    }

    /// Set maximum number of selections allowed
    pub fn max_selections(mut self, max: usize) -> Self {
        self.max_selections = Some(max);
        self
    }

    /// Use a custom prompt config
    pub fn with_config(mut self, config: PromptConfig) -> Self {
        self.config = Some(config);
        self
    }

    fn render_options(&self, selected: &[bool]) -> String {
        let mut output = String::new();
        output.push_str(&self.message);
        output.push('\n');

        for (i, (label, _)) in self.options.iter().enumerate() {
            let is_selected = selected.get(i).copied().unwrap_or(false);
            let is_default = self.default_indices.contains(&i);
            let marker = if is_selected { "[x]" } else { "[ ]" };
            let default_mark = if is_default { " (default)" } else { "" };
            output.push_str(&format!(
                "  {} {}. {}{}\n",
                marker,
                i + 1,
                label,
                default_mark
            ));
        }

        output
    }

    fn parse_input(&self, input: &str, current_selected: &mut [bool]) -> bool {
        let input = input.trim();
        let mut changed = false;

        // Parse comma-separated numbers or ranges
        for part in input.split(',') {
            let part = part.trim();
            if part.contains('-') {
                // Range: 1-3
                let range_parts: Vec<&str> = part.split('-').collect();
                if range_parts.len() == 2 {
                    if let (Ok(start), Ok(end)) = (
                        range_parts[0].parse::<usize>(),
                        range_parts[1].parse::<usize>(),
                    ) {
                        let lo = start.saturating_sub(1).min(current_selected.len());
                        let hi = end.min(current_selected.len());
                        for sel in current_selected[lo..hi].iter_mut() {
                            *sel = !*sel;
                            changed = true;
                        }
                    }
                }
            } else if let Ok(index) = part.parse::<usize>() {
                // Single number
                if index >= 1 && index <= self.options.len() {
                    current_selected[index - 1] = !current_selected[index - 1];
                    changed = true;
                }
            }
        }

        changed
    }

    fn validate_selections(&self, selected: &[bool]) -> Result<(), String> {
        let count = selected.iter().filter(|&&s| s).count();

        if count < self.min_selections {
            return Err(format!(
                "At least {} selection(s) required. You have selected {}.",
                self.min_selections, count
            ));
        }

        if let Some(max) = self.max_selections {
            if count > max {
                return Err(format!(
                    "At most {} selection(s) allowed. You have selected {}.",
                    max, count
                ));
            }
        }

        Ok(())
    }
}

impl<T: Clone> Prompt for MultiSelect<T> {
    type Output = Vec<T>;

    fn prompt(self) -> io::Result<Self::Output> {
        let config = self
            .config
            .clone()
            .unwrap_or_else(|| PromptConfig::global().clone());
        self.prompt_with_config(&config)
    }

    fn prompt_with_config(self, config: &PromptConfig) -> io::Result<Self::Output> {
        // Initialize selection state
        let mut selected: Vec<bool> = vec![false; self.options.len()];
        for &index in &self.default_indices {
            if index < self.options.len() {
                selected[index] = true;
            }
        }

        // If global yes is enabled, return defaults without prompting
        if config.global_yes_enabled() {
            let values: Vec<T> = self
                .options
                .iter()
                .enumerate()
                .filter(|(i, _)| selected[*i])
                .map(|(_, (_, value))| value.clone())
                .collect();
            return Ok(values);
        }

        let mut stdout = io::stdout().lock();

        loop {
            // Render current selection state
            let prompt_text = self.render_options(&selected);
            write!(stdout, "{}", prompt_text)?;
            write!(stdout, "Toggle selections (e.g., 1,2-3), empty to submit: ")?;
            stdout.flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;

            // Empty input submits the selection
            if input.trim().is_empty() {
                if let Err(err) = self.validate_selections(&selected) {
                    writeln!(io::stderr(), "{}", err)?;
                    continue;
                }

                let values: Vec<T> = self
                    .options
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| selected[*i])
                    .map(|(_, (_, value))| value.clone())
                    .collect();

                return Ok(values);
            }

            // Parse and update selections
            self.parse_input(&input, &mut selected);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_config_global_yes() {
        let config = PromptConfig::new();
        assert!(!config.global_yes_enabled());

        config.set_global_yes(true);
        assert!(config.global_yes_enabled());

        config.set_global_yes(false);
        assert!(!config.global_yes_enabled());
    }

    #[test]
    fn test_confirm_parse_input() {
        let confirm = Confirm::new("Test");
        assert_eq!(confirm.parse_input("y"), Some(true));
        assert_eq!(confirm.parse_input("Y"), Some(true));
        assert_eq!(confirm.parse_input("yes"), Some(true));
        assert_eq!(confirm.parse_input("YES"), Some(true));
        assert_eq!(confirm.parse_input("n"), Some(false));
        assert_eq!(confirm.parse_input("N"), Some(false));
        assert_eq!(confirm.parse_input("no"), Some(false));
        assert_eq!(confirm.parse_input("NO"), Some(false));
        assert_eq!(confirm.parse_input(""), Some(false)); // default
        assert_eq!(confirm.parse_input("invalid"), None);
    }

    #[test]
    fn test_confirm_with_default() {
        let confirm = Confirm::new("Test").with_default(true);
        assert!(confirm.default);
        assert_eq!(confirm.parse_input(""), Some(true)); // default
    }

    #[test]
    fn test_select_parse_input() {
        let select = Select::<&str>::new("Test")
            .option("Option 1", "a")
            .option("Option 2", "b")
            .option("Option 3", "c");

        assert_eq!(select.parse_input("1"), Some(0));
        assert_eq!(select.parse_input("2"), Some(1));
        assert_eq!(select.parse_input("3"), Some(2));
        assert_eq!(select.parse_input("0"), None);
        assert_eq!(select.parse_input("4"), None);
        assert_eq!(select.parse_input("invalid"), None);
    }

    #[test]
    fn test_multiselect_parse_input() {
        let multiselect = MultiSelect::<&str>::new("Test")
            .option("Option 1", "a")
            .option("Option 2", "b")
            .option("Option 3", "c");

        let mut selected = vec![false; 3];

        // Single selection
        multiselect.parse_input("1", &mut selected);
        assert_eq!(selected, vec![true, false, false]);

        // Range selection
        multiselect.parse_input("1-3", &mut selected);
        assert_eq!(selected, vec![false, true, true]);

        // Multiple selections
        multiselect.parse_input("1,3", &mut selected);
        assert_eq!(selected, vec![true, true, false]);
    }

    #[test]
    fn test_multiselect_validation() {
        let multiselect = MultiSelect::<&str>::new("Test")
            .option("Option 1", "a")
            .option("Option 2", "b")
            .option("Option 3", "c")
            .min_selections(1)
            .max_selections(2);

        // Too few selections
        assert!(multiselect
            .validate_selections(&[false, false, false])
            .is_err());

        // Valid selection
        assert!(multiselect
            .validate_selections(&[true, false, false])
            .is_ok());

        // Too many selections
        assert!(multiselect
            .validate_selections(&[true, true, true])
            .is_err());
    }

    #[test]
    fn test_validation_error() {
        let err = ValidationError {
            message: "Test error".to_string(),
        };
        assert_eq!(err.to_string(), "Test error");

        let err2 = ValidationError::from("String error");
        assert_eq!(err2.to_string(), "String error");

        let err3 = ValidationError::from("From str error");
        assert_eq!(err3.to_string(), "From str error");
    }

    #[test]
    fn test_confirm_default_constructor() {
        let confirm = Confirm::new("Continue?");
        assert_eq!(confirm.message, "Continue?");
        assert!(!confirm.default);
    }

    #[test]
    fn test_confirm_default_false() {
        let confirm = Confirm::new("Test");
        assert_eq!(confirm.parse_input(""), Some(false));
    }

    #[test]
    fn test_confirm_render_prompt() {
        let confirm_default = Confirm::new("Proceed");
        assert_eq!(confirm_default.render_prompt(), "Proceed [y/N]: ");

        let confirm_yes = Confirm::new("Proceed").with_default(true);
        assert_eq!(confirm_yes.render_prompt(), "Proceed [Y/n]: ");
    }

    #[test]
    fn test_prompt_config_default() {
        let config = PromptConfig::default();
        assert!(!config.global_yes_enabled());
    }

    #[test]
    fn test_prompt_config_clone() {
        let config = PromptConfig::new();
        let cloned = config.clone();
        config.set_global_yes(true);
        // Clone shares the same Arc<AtomicBool>
        assert!(cloned.global_yes_enabled());
        config.set_global_yes(false);
    }

    #[test]
    fn test_select_options() {
        let select = Select::<&str>::new("Pick")
            .option("A", "a")
            .option("B", "b");

        assert_eq!(select.options.len(), 2);
        assert_eq!(select.options[0].0, "A");
        assert_eq!(select.options[1].1, "b");
    }

    #[test]
    fn test_multiselect_options() {
        let ms = MultiSelect::<i32>::new("Pick numbers")
            .option("One", 1)
            .option("Two", 2);

        assert_eq!(ms.options.len(), 2);
    }

    #[test]
    fn test_multiselect_range_selection() {
        let ms = MultiSelect::<&str>::new("Test")
            .option("A", "a")
            .option("B", "b")
            .option("C", "c")
            .option("D", "d");

        let mut selected = vec![false; 4];
        ms.parse_input("2-3", &mut selected);
        assert_eq!(selected, vec![false, true, true, false]);
    }

    #[test]
    fn test_multiselect_comma_selection() {
        let ms = MultiSelect::<&str>::new("Test")
            .option("A", "a")
            .option("B", "b")
            .option("C", "c");

        let mut selected = vec![false; 3];
        ms.parse_input("1,3", &mut selected);
        assert_eq!(selected, vec![true, false, true]);
    }

    #[test]
    fn test_multiselect_min_max_defaults() {
        let ms = MultiSelect::<&str>::new("Test")
            .option("A", "a")
            .option("B", "b");
        // Default: min_selections=0, max_selections=None
        assert_eq!(ms.min_selections, 0);
        assert!(ms.max_selections.is_none());
    }

    #[test]
    fn test_select_invalid_indices() {
        let select = Select::<&str>::new("Test")
            .option("A", "a")
            .option("B", "b");

        assert_eq!(select.parse_input("0"), None);
        assert_eq!(select.parse_input("3"), None);
        assert_eq!(select.parse_input("abc"), None);
        assert_eq!(select.parse_input("-1"), None);
    }

    #[test]
    fn test_validation_error_from_string() {
        let msg = String::from("custom error");
        let err = ValidationError::from(msg);
        assert_eq!(err.message, "custom error");
    }

    #[test]
    fn test_confirm_with_config() {
        let config = PromptConfig::new();
        let confirm = Confirm::new("Test").with_config(config);
        assert!(confirm.config.is_some());
    }

    #[test]
    fn test_prompt_config_global_instance() {
        // Global instance should be accessible and return a bool
        let global = PromptConfig::global();
        let _initial = global.global_yes_enabled();
        // Just verify it's accessible and doesn't panic
    }

    #[test]
    fn test_input_default_constructor() {
        let input = Input::new("Name");
        assert_eq!(input.message, "Name");
        assert!(input.default.is_none());
    }

    #[test]
    fn test_input_with_default_value() {
        let input = Input::new("Name").with_default("Guest");
        assert_eq!(input.default, Some("Guest".to_string()));
    }

    #[test]
    fn test_input_render_prompt_with_default() {
        let input = Input::new("Name").with_default("Guest");
        assert_eq!(input.render_prompt(), "Name [Guest]: ");
    }

    #[test]
    fn test_input_render_prompt_without_default() {
        let input = Input::new("Name");
        assert_eq!(input.render_prompt(), "Name: ");
    }

    #[test]
    fn test_input_with_config() {
        let config = PromptConfig::new();
        let input = Input::new("Test").with_config(config);
        assert!(input.config.is_some());
    }

    #[test]
    fn test_input_with_validator() {
        let input = Input::new("Age").validate(|s| {
            if s.parse::<u32>().is_err() {
                Err(ValidationError::from("Must be a number"))
            } else {
                Ok(())
            }
        });
        assert!(input.validator.is_some());
    }

    #[test]
    fn test_confirm_non_interactive_returns_default() {
        let config = PromptConfig::new();
        config.set_global_yes(true);

        let confirm = Confirm::new("Continue?")
            .with_default(true)
            .with_config(config.clone());
        let result = confirm.prompt().unwrap();
        assert!(result);

        let confirm2 = Confirm::new("Continue?")
            .with_default(false)
            .with_config(config);
        let result2 = confirm2.prompt().unwrap();
        assert!(!result2);
    }

    #[test]
    fn test_select_non_interactive_returns_default() {
        let config = PromptConfig::new();
        config.set_global_yes(true);

        let select = Select::<&str>::new("Pick")
            .option("A", "a")
            .option("B", "b")
            .with_default(1)
            .with_config(config);
        let result = select.prompt().unwrap();
        assert_eq!(result, "b");
    }

    #[test]
    fn test_select_non_interactive_default_first() {
        let config = PromptConfig::new();
        config.set_global_yes(true);

        let select = Select::<&str>::new("Pick")
            .option("A", "a")
            .option("B", "b")
            .with_config(config);
        let result = select.prompt().unwrap();
        assert_eq!(result, "a"); // default is first when no explicit default
    }

    #[test]
    fn test_input_non_interactive_returns_default() {
        let config = PromptConfig::new();
        config.set_global_yes(true);

        let input = Input::new("Name").with_default("Guest").with_config(config);
        let result = input.prompt().unwrap();
        assert_eq!(result, "Guest");
    }

    #[test]
    fn test_input_non_interactive_no_default() {
        let config = PromptConfig::new();
        config.set_global_yes(true);

        let input = Input::new("Name").with_config(config);
        let result = input.prompt().unwrap();
        assert_eq!(result, ""); // empty string default
    }

    #[test]
    fn test_multiselect_non_interactive_returns_defaults() {
        let config = PromptConfig::new();
        config.set_global_yes(true);

        let ms = MultiSelect::<&str>::new("Pick")
            .option("A", "a")
            .option("B", "b")
            .option("C", "c")
            .with_defaults(vec![0, 2])
            .with_config(config);
        let result = ms.prompt().unwrap();
        assert_eq!(result, vec!["a", "c"]);
    }

    #[test]
    fn test_multiselect_non_interactive_no_defaults() {
        let config = PromptConfig::new();
        config.set_global_yes(true);

        let ms = MultiSelect::<&str>::new("Pick")
            .option("A", "a")
            .option("B", "b")
            .with_config(config);
        let result = ms.prompt().unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_select_render_options() {
        let select = Select::<&str>::new("Choose")
            .option("Red", "red")
            .option("Blue", "blue")
            .with_default(0);

        let rendered = select.render_options();
        assert!(rendered.contains("Choose"));
        assert!(rendered.contains("> 1. Red"));
        assert!(rendered.contains(" 2. Blue"));
    }

    #[test]
    fn test_multiselect_render_options() {
        let ms = MultiSelect::<&str>::new("Pick")
            .option("A", "a")
            .option("B", "b")
            .with_defaults(vec![0]);

        let selected = vec![true, false];
        let rendered = ms.render_options(&selected);
        assert!(rendered.contains("Pick"));
        assert!(rendered.contains("[x] 1. A"));
        assert!(rendered.contains("[ ] 2. B"));
        assert!(rendered.contains("(default)"));
    }

    #[test]
    fn test_select_with_validator() {
        let select = Select::<i32>::new("Pick number")
            .option("One", 1)
            .option("Two", 2)
            .validate(|v| {
                if *v > 1 {
                    Ok(())
                } else {
                    Err(ValidationError::from("Must be greater than 1"))
                }
            });
        assert!(select.validator.is_some());
    }

    #[test]
    fn test_select_with_config() {
        let config = PromptConfig::new();
        let select = Select::<&str>::new("Test")
            .option("A", "a")
            .with_config(config);
        assert!(select.config.is_some());
    }

    #[test]
    fn test_select_options_batch() {
        let select = Select::<&str>::new("Pick").options([("X", "x"), ("Y", "y"), ("Z", "z")]);
        assert_eq!(select.options.len(), 3);
        assert_eq!(select.options[0].0, "X");
        assert_eq!(select.options[2].1, "z");
    }

    #[test]
    fn test_multiselect_options_batch() {
        let ms = MultiSelect::<i32>::new("Nums").options([("One", 1), ("Two", 2), ("Three", 3)]);
        assert_eq!(ms.options.len(), 3);
    }

    #[test]
    fn test_multiselect_with_min_max() {
        let ms = MultiSelect::<&str>::new("Test")
            .option("A", "a")
            .option("B", "b")
            .min_selections(1)
            .max_selections(1);
        assert_eq!(ms.min_selections, 1);
        assert_eq!(ms.max_selections, Some(1));
    }

    #[test]
    fn test_multiselect_validate_boundary() {
        let ms = MultiSelect::<&str>::new("Test")
            .option("A", "a")
            .option("B", "b")
            .min_selections(1)
            .max_selections(1);

        // Exactly 1 — valid
        assert!(ms.validate_selections(&[true, false]).is_ok());
        assert!(ms.validate_selections(&[false, true]).is_ok());

        // 0 or 2 — invalid
        assert!(ms.validate_selections(&[false, false]).is_err());
        assert!(ms.validate_selections(&[true, true]).is_err());
    }

    #[test]
    fn test_multiselect_parse_input_empty() {
        let ms = MultiSelect::<&str>::new("Test")
            .option("A", "a")
            .option("B", "b");

        let mut selected = vec![false, false];
        let changed = ms.parse_input("", &mut selected);
        assert!(!changed);
        assert_eq!(selected, vec![false, false]);
    }

    #[test]
    fn test_multiselect_parse_input_toggle() {
        let ms = MultiSelect::<&str>::new("Test")
            .option("A", "a")
            .option("B", "b");

        let mut selected = vec![true, false];
        ms.parse_input("1", &mut selected);
        assert_eq!(selected, vec![false, false]); // toggled off
    }

    #[test]
    fn test_confirm_parse_whitespace() {
        let confirm = Confirm::new("Test");
        assert_eq!(confirm.parse_input("  y  "), Some(true));
        assert_eq!(confirm.parse_input("\t n\t"), Some(false));
    }

    #[test]
    fn test_select_parse_whitespace() {
        let select = Select::<&str>::new("Test")
            .option("A", "a")
            .option("B", "b");
        assert_eq!(select.parse_input("  1  "), Some(0));
        assert_eq!(select.parse_input("\t2\t"), Some(1));
    }

    #[test]
    fn test_multiselect_parse_out_of_range() {
        let ms = MultiSelect::<&str>::new("Test")
            .option("A", "a")
            .option("B", "b");

        let mut selected = vec![false, false];
        ms.parse_input("5", &mut selected); // out of range — ignored
        assert_eq!(selected, vec![false, false]);

        ms.parse_input("0", &mut selected); // 0 is not valid
        assert_eq!(selected, vec![false, false]);
    }

    #[test]
    fn test_multiselect_parse_invalid_range() {
        let ms = MultiSelect::<&str>::new("Test")
            .option("A", "a")
            .option("B", "b");

        let mut selected = vec![false, false];
        ms.parse_input("abc-def", &mut selected);
        assert_eq!(selected, vec![false, false]); // invalid range ignored
    }

    #[test]
    fn test_prompt_config_set_global_yes_enabled() {
        let initial = PromptConfig::global().global_yes_enabled();
        PromptConfig::set_global_yes_enabled(true);
        assert!(PromptConfig::global().global_yes_enabled());
        PromptConfig::set_global_yes_enabled(initial);
    }

    #[test]
    fn test_validation_error_debug() {
        let err = ValidationError {
            message: "test".to_string(),
        };
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("test"));
    }

    #[test]
    fn test_prompt_config_thread_safety() {
        use std::thread;

        let config = PromptConfig::new();
        let config_clone = config.clone();

        config.set_global_yes(true);
        let handle = thread::spawn(move || config_clone.global_yes_enabled());
        assert!(handle.join().unwrap());
        config.set_global_yes(false);
    }
}
