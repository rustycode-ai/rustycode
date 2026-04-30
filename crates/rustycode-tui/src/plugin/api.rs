//! Plugin API exposed to plugins

use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

/// Result type for plugin commands
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum CommandResult {
    /// Command succeeded with a message
    Message(String),

    /// Command returned data
    Data(String),

    /// Command failed with an error
    Error(String),

    /// Command succeeded silently
    Ok,
}

/// Plugin command handler function type
pub type CommandHandler = fn(&mut PluginAPI, Vec<String>) -> CommandResult;

/// Callback types for plugin UI and context
pub type MessageSenderCallback = Arc<Mutex<Option<Box<dyn Fn(String) + Send>>>>;
pub type InputGetterCallback = Arc<Mutex<Option<Box<dyn Fn() -> String + Send>>>>;
pub type InputSetterCallback = Arc<Mutex<Option<Box<dyn Fn(String) + Send>>>>;
pub type WorkspaceGetterCallback = Arc<Mutex<Option<Box<dyn Fn() -> String + Send>>>>;
pub type CwdGetterCallback = Arc<Mutex<Option<Box<dyn Fn() -> std::path::PathBuf + Send>>>>;
pub type HistoryGetterCallback = Arc<Mutex<Option<Box<dyn Fn() -> Vec<String> + Send>>>>;

/// Plugin API - provides safe access to TUI functionality
pub struct PluginAPI {
    /// Plugin name
    pub plugin_name: String,

    /// Configuration
    pub config: PluginConfig,

    /// UI control
    pub ui: PluginUI,

    /// Commands
    pub commands: PluginCommands,

    /// Context access
    pub context: PluginContext,
}

impl PluginAPI {
    /// Create new PluginAPI
    pub fn new(plugin_name: String) -> Self {
        Self {
            plugin_name,
            config: PluginConfig::new(),
            ui: PluginUI::new(),
            commands: PluginCommands::new(),
            context: PluginContext::new(),
        }
    }

    /// Show a message to the user
    pub fn show_message(&self, message: &str) {
        self.ui.show_message(message);
    }

    /// Get current input text
    pub fn get_input(&self) -> String {
        self.ui.get_input()
    }

    /// Set input text
    pub fn set_input(&self, text: &str) {
        self.ui.set_input(text);
    }

    /// Register a slash command
    pub fn register_command(&mut self, name: String, handler: CommandHandler) {
        self.commands.register(name, handler);
    }

    /// Get plugin configuration value
    pub fn get_config(&self, key: &str) -> Option<String> {
        self.config.get(key)
    }

    /// Set plugin configuration value
    pub fn set_config(&mut self, key: String, value: String) {
        self.config.set(key, value);
    }
}

/// Plugin configuration
#[derive(Clone)]
pub struct PluginConfig {
    /// Configuration values
    values: Arc<Mutex<std::collections::HashMap<String, String>>>,
}

impl Default for PluginConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginConfig {
    pub fn new() -> Self {
        Self {
            values: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }
    }

    pub fn get(&self, key: &str) -> Option<String> {
        self.values
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(key)
            .cloned()
    }

    pub fn set(&self, key: String, value: String) {
        self.values
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(key, value);
    }

    pub fn load_from_file(&self, path: &std::path::Path) -> Result<(), anyhow::Error> {
        let content = std::fs::read_to_string(path)?;
        let config: std::collections::HashMap<String, String> = toml::from_str(&content)?;

        let mut values = self.values.lock().unwrap_or_else(|e| e.into_inner());
        for (key, value) in config {
            values.insert(key, value);
        }

        Ok(())
    }

    pub fn save_to_file(&self, path: &std::path::Path) -> Result<(), anyhow::Error> {
        let values = self.values.lock().unwrap_or_else(|e| e.into_inner());
        let content = toml::to_string_pretty(&*values)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}

/// Plugin UI control
#[derive(Clone)]
pub struct PluginUI {
    /// Message sender callback
    message_sender: MessageSenderCallback,

    /// Input getter callback
    input_getter: InputGetterCallback,

    /// Input setter callback
    input_setter: InputSetterCallback,
}

impl Default for PluginUI {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginUI {
    pub fn new() -> Self {
        Self {
            message_sender: Arc::new(Mutex::new(None)),
            input_getter: Arc::new(Mutex::new(None)),
            input_setter: Arc::new(Mutex::new(None)),
        }
    }

    pub fn show_message(&self, message: &str) {
        if let Some(sender) = self
            .message_sender
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            sender(message.to_string());
        }
    }

    pub fn get_input(&self) -> String {
        if let Some(getter) = self
            .input_getter
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            getter()
        } else {
            String::new()
        }
    }

    pub fn set_input(&self, text: &str) {
        if let Some(setter) = self
            .input_setter
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            setter(text.to_string());
        }
    }

    /// Set the message sender callback
    pub fn set_message_sender(&mut self, sender: Box<dyn Fn(String) + Send>) {
        *self
            .message_sender
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(sender);
    }

    /// Set the input getter callback
    pub fn set_input_getter(&mut self, getter: Box<dyn Fn() -> String + Send>) {
        *self.input_getter.lock().unwrap_or_else(|e| e.into_inner()) = Some(getter);
    }

    /// Set the input setter callback
    pub fn set_input_setter(&mut self, setter: Box<dyn Fn(String) + Send>) {
        *self.input_setter.lock().unwrap_or_else(|e| e.into_inner()) = Some(setter);
    }
}

/// Plugin command registration
#[derive(Clone)]
pub struct PluginCommands {
    /// Registered commands
    commands: Arc<Mutex<std::collections::HashMap<String, CommandHandler>>>,
}

impl Default for PluginCommands {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginCommands {
    pub fn new() -> Self {
        Self {
            commands: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }
    }

    pub fn register(&mut self, name: String, handler: CommandHandler) {
        self.commands
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(name, handler);
    }

    pub fn get(&self, name: &str) -> Option<CommandHandler> {
        self.commands
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(name)
            .copied()
    }

    pub fn list(&self) -> Vec<String> {
        self.commands
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .keys()
            .cloned()
            .collect()
    }
}

/// Plugin context access
#[derive(Clone)]
pub struct PluginContext {
    /// Workspace context getter
    workspace_getter: WorkspaceGetterCallback,

    /// Current working directory getter
    cwd_getter: CwdGetterCallback,

    /// Conversation history getter
    history_getter: HistoryGetterCallback,
}

impl Default for PluginContext {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginContext {
    pub fn new() -> Self {
        Self {
            workspace_getter: Arc::new(Mutex::new(None)),
            cwd_getter: Arc::new(Mutex::new(None)),
            history_getter: Arc::new(Mutex::new(None)),
        }
    }

    /// Get workspace context
    pub fn get_workspace(&self) -> String {
        if let Some(getter) = self
            .workspace_getter
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            getter()
        } else {
            String::new()
        }
    }

    /// Get current working directory
    pub fn get_cwd(&self) -> std::path::PathBuf {
        if let Some(getter) = self
            .cwd_getter
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            getter()
        } else {
            std::env::current_dir().unwrap_or_default()
        }
    }

    /// Get conversation history
    pub fn get_history(&self) -> Vec<String> {
        if let Some(getter) = self
            .history_getter
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            getter()
        } else {
            Vec::new()
        }
    }

    /// Set workspace context getter
    pub fn set_workspace_getter(&mut self, getter: Box<dyn Fn() -> String + Send>) {
        *self
            .workspace_getter
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(getter);
    }

    /// Set current working directory getter
    pub fn set_cwd_getter(&mut self, getter: Box<dyn Fn() -> std::path::PathBuf + Send>) {
        *self.cwd_getter.lock().unwrap_or_else(|e| e.into_inner()) = Some(getter);
    }

    /// Set conversation history getter
    pub fn set_history_getter(&mut self, getter: Box<dyn Fn() -> Vec<String> + Send>) {
        *self
            .history_getter
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(getter);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_result() {
        let result = CommandResult::Message("Success".to_string());
        assert!(matches!(result, CommandResult::Message(_)));

        let error = CommandResult::Error("Failed".to_string());
        assert!(matches!(error, CommandResult::Error(_)));
    }

    #[test]
    fn test_plugin_config() {
        let config = PluginConfig::new();
        config.set("key1".to_string(), "value1".to_string());
        assert_eq!(config.get("key1"), Some("value1".to_string()));
        assert_eq!(config.get("key2"), None);
    }

    #[test]
    fn test_plugin_commands() {
        let mut commands = PluginCommands::new();

        let handler: CommandHandler = |_api, _args| CommandResult::Ok;
        commands.register("test".to_string(), handler);

        assert!(commands.get("test").is_some());
        assert!(commands.get("nonexistent").is_none());

        let list = commands.list();
        assert_eq!(list.len(), 1);
        assert!(list.contains(&"test".to_string()));
    }

    #[test]
    fn test_plugin_api() {
        let mut api = PluginAPI::new("test-plugin".to_string());

        // Test config
        api.set_config("key".to_string(), "value".to_string());
        assert_eq!(api.get_config("key"), Some("value".to_string()));

        // Test commands
        let handler: CommandHandler = |_api, _args| CommandResult::Message("Test".to_string());
        api.register_command("cmd".to_string(), handler);
    }
}
