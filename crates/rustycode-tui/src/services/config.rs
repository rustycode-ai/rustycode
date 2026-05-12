//! TUI configuration management

use rustycode_config::paths::RustyCodePath;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Comprehensive TUI configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TUIConfig {
    pub theme: ThemeConfig,
    pub keybindings: KeyBindings,
    pub ui: UIConfig,
    pub behavior: BehaviorConfig,
    #[serde(default)]
    pub renderer: crate::app::renderer::RendererConfig,
}

/// Theme configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeConfig {
    pub name: String,
    pub custom_colors: Option<ColorPaletteOverride>,
}

/// Override specific colors in a theme
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorPaletteOverride {
    pub background: Option<String>,
    pub foreground: Option<String>,
    pub primary: Option<String>,
    pub secondary: Option<String>,
    pub accent: Option<String>,
    pub success: Option<String>,
    pub warning: Option<String>,
    pub error: Option<String>,
    pub muted: Option<String>,
}

/// Keybinding configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyBindings {
    pub quit: Vec<String>,
    pub save: Vec<String>,
    pub search: Vec<String>,
    pub theme_switch: Vec<String>,
    pub help: Vec<String>,
    pub clear: Vec<String>,
}

/// UI preferences
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UIConfig {
    pub font_size: u8,
    pub line_height: u8,
    pub padding: u16,
    pub show_line_numbers: bool,
    #[serde(default = "default_show_status_bar")]
    pub show_status_bar: bool,
    pub show_tool_panel: bool,
    pub message_compact: bool,
    /// Enable brutalist renderer mode (asymmetric borders, compact layout)
    #[serde(default = "default_brutalist_mode")]
    pub brutalist_mode: bool,
}

/// Behavior settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorConfig {
    pub auto_save_interval_seconds: u64,
    pub max_history_size: usize,
    pub confirm_on_dangerous: bool,
    pub yolo_mode: bool,
    pub auto_scroll: bool,
    pub stream_responses: bool,
    /// Mouse wheel scroll speed in lines per tick (1-10, default: 3)
    #[serde(default = "default_mouse_scroll_speed")]
    pub mouse_scroll_speed: u8,
    /// Enable Vim-style keybindings (default: false)
    #[serde(default)]
    pub vim_enabled: bool,
    /// Disable animations for accessibility (default: false)
    #[serde(default)]
    pub reduced_motion: bool,
}

/// Default mouse scroll speed
fn default_mouse_scroll_speed() -> u8 {
    3
}

/// Default show status bar
fn default_show_status_bar() -> bool {
    true
}

/// Default brutalist mode
fn default_brutalist_mode() -> bool {
    true // Enable brutalist mode by default
}

impl BehaviorConfig {
    /// Validate and constrain mouse scroll speed to valid bounds (1-10)
    pub fn validate_scroll_speed(value: u8) -> u8 {
        value.clamp(1, 10)
    }

    /// Set mouse scroll speed with validation
    pub fn set_mouse_scroll_speed(&mut self, value: u8) {
        self.mouse_scroll_speed = Self::validate_scroll_speed(value);
    }

    /// Get validated mouse scroll speed
    pub fn mouse_scroll_speed(&self) -> u8 {
        self.mouse_scroll_speed.clamp(1, 10)
    }
}

impl Default for TUIConfig {
    fn default() -> Self {
        Self {
            theme: ThemeConfig {
                name: "tokyo-night".to_string(),
                custom_colors: None,
            },
            keybindings: KeyBindings {
                quit: vec!["Ctrl+c".to_string(), "q".to_string()],
                save: vec!["Ctrl+s".to_string()],
                search: vec!["Ctrl+f".to_string()],
                theme_switch: vec!["Ctrl+t".to_string()],
                help: vec!["F1".to_string()],
                clear: vec!["Ctrl+l".to_string()],
            },
            ui: UIConfig {
                font_size: 14,
                line_height: 16,
                padding: 8,
                show_line_numbers: true,
                show_status_bar: true,
                show_tool_panel: true,
                message_compact: false,
                brutalist_mode: true,
            },
            behavior: BehaviorConfig {
                auto_save_interval_seconds: 30,
                max_history_size: 1000,
                confirm_on_dangerous: true,
                yolo_mode: false,
                auto_scroll: true,
                stream_responses: true,
                mouse_scroll_speed: 3,
                vim_enabled: false,
                reduced_motion: false,
            },
            renderer: crate::app::renderer::RendererConfig::default(),
        }
    }
}

pub fn get_config_path() -> PathBuf {
    RustyCodePath::tui_config_file().unwrap_or_else(|_| PathBuf::from(".rustycode/tui-config.json"))
}

pub fn profiles_dir() -> PathBuf {
    RustyCodePath::config_dir()
        .map(|p| p.join("profiles"))
        .unwrap_or_else(|_| PathBuf::from(".rustycode/profiles"))
}

pub fn config_schema_path() -> PathBuf {
    RustyCodePath::config_dir()
        .map(|p| p.join("tui-config.schema.json"))
        .unwrap_or_else(|_| PathBuf::from(".rustycode/tui-config.schema.json"))
}

/// Load configuration from disk
pub fn load_config() -> TUIConfig {
    let path = get_config_path();
    if path.exists() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(config_value) = serde_json::from_str::<serde_json::Value>(&content) {
                let validator = rustycode_config::schema::SchemaValidator::new();
                if validator
                    .validate_with_schema_name(&config_value, "tui")
                    .is_ok()
                {
                    if let Ok(config) = serde_json::from_value::<TUIConfig>(config_value) {
                        return config;
                    }
                }
            }
        }
    }
    TUIConfig::default()
}

/// Save configuration to disk
pub fn save_config(config: &TUIConfig) -> std::io::Result<()> {
    let path = get_config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(config)?;
    std::fs::write(&path, content)
}

/// Remap a keybinding
pub fn remap_key(config: &mut TUIConfig, action: String, keys: Vec<String>) -> Result<(), String> {
    match action.as_str() {
        "quit" => config.keybindings.quit = keys,
        "save" => config.keybindings.save = keys,
        "search" => config.keybindings.search = keys,
        "theme_switch" => config.keybindings.theme_switch = keys,
        "help" => config.keybindings.help = keys,
        "clear" => config.keybindings.clear = keys,
        _ => return Err(format!("Unknown action: {}", action)),
    }
    Ok(())
}

/// Profile manager for saving and loading configurations
pub struct ProfileManager {
    pub profiles: HashMap<String, TUIConfig>,
    pub current_profile: String,
}

impl ProfileManager {
    pub fn new() -> Self {
        let mut profiles = HashMap::new();
        profiles.insert("default".to_string(), TUIConfig::default());

        // Load existing profiles from disk
        let profiles_dir = profiles_dir();
        if profiles_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&profiles_dir) {
                for entry in entries.filter_map(|e| e.ok()) {
                    let path = entry.path();
                    if path.extension().and_then(|s| s.to_str()) == Some("json") {
                        if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                            if let Ok(content) = std::fs::read_to_string(&path) {
                                if let Ok(config) = serde_json::from_str::<TUIConfig>(&content) {
                                    profiles.insert(name.to_string(), config);
                                }
                            }
                        }
                    }
                }
            }
        }

        Self {
            profiles,
            current_profile: "default".to_string(),
        }
    }

    pub fn save_profile(&mut self, name: String, config: TUIConfig) -> Result<(), String> {
        let profiles_dir = profiles_dir();
        std::fs::create_dir_all(&profiles_dir)
            .map_err(|e| format!("Failed to create profiles directory: {}", e))?;

        let profile_path = profiles_dir.join(format!("{}.json", name));
        let content = serde_json::to_string_pretty(&config)
            .map_err(|e| format!("Failed to serialize profile: {}", e))?;

        std::fs::write(&profile_path, content)
            .map_err(|e| format!("Failed to write profile: {}", e))?;

        self.profiles.insert(name.clone(), config);
        Ok(())
    }

    pub fn load_profile(&mut self, name: &str) -> Result<TUIConfig, String> {
        if let Some(config) = self.profiles.get(name) {
            self.current_profile = name.to_string();
            Ok(config.clone())
        } else {
            Err(format!("Profile '{}' not found", name))
        }
    }

    pub fn list_profiles(&self) -> Vec<String> {
        self.profiles.keys().cloned().collect()
    }
}

impl Default for ProfileManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Legacy model configuration (kept for backward compatibility)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub current_provider: String,
    pub current_model: String,
    pub temperature: f32,
    pub max_tokens: usize,
    pub top_p: f32,
    pub stream: bool,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            current_provider: "anthropic".to_string(),
            current_model: "claude-sonnet-4-6".to_string(),
            temperature: 0.7,
            max_tokens: 4096,
            top_p: 1.0,
            stream: true,
            effort: None,
        }
    }
}

impl ModelConfig {
    /// Validate configuration parameters.
    ///
    pub fn is_valid(&self) -> bool {
        self.temperature >= 0.0
            && self.temperature <= 1.0
            && self.top_p >= 0.0
            && self.top_p <= 1.0
            && self.max_tokens > 0
            && !self.current_model.is_empty()
            && !self.current_provider.is_empty()
    }

    /// Get temperature as formatted string.
    pub fn temperature_display(&self) -> String {
        format!("{:.1}", self.temperature)
    }

    /// Get max tokens as formatted string.
    pub fn max_tokens_display(&self) -> String {
        if self.max_tokens >= 1000 {
            format!("{}k", self.max_tokens / 1000)
        } else {
            format!("{}", self.max_tokens)
        }
    }
}

pub fn model_config_path() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".rustycode/config.json"))
        .unwrap_or_else(|| PathBuf::from(".rustycode/config.json"))
}

pub fn load_model_config() -> ModelConfig {
    let path = model_config_path();
    if path.exists() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(config) = serde_json::from_str::<ModelConfig>(&content) {
                return config;
            }
        }
    }
    ModelConfig::default()
}

pub fn save_model_config(config: &ModelConfig) -> std::io::Result<()> {
    let path = model_config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(config)?;
    std::fs::write(&path, content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tui_config_default() {
        let config = TUIConfig::default();
        assert_eq!(config.theme.name, "tokyo-night");
        assert!(config.keybindings.quit.contains(&"Ctrl+c".to_string()));
        assert_eq!(config.ui.padding, 8);
        assert_eq!(config.behavior.auto_save_interval_seconds, 30);
        assert_eq!(config.renderer.min_width, 40);
    }

    #[test]
    fn test_remap_key() {
        let mut config = TUIConfig::default();
        remap_key(&mut config, "quit".to_string(), vec!["Ctrl+d".to_string()]).unwrap();
        assert_eq!(config.keybindings.quit, vec!["Ctrl+d".to_string()]);

        let result = remap_key(&mut config, "unknown".to_string(), vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn test_profile_manager() {
        let mut manager = ProfileManager::new();
        assert_eq!(manager.current_profile, "default");

        let config = TUIConfig::default();
        manager.save_profile("test".to_string(), config).unwrap();

        let loaded = manager.load_profile("test").unwrap();
        assert_eq!(loaded.theme.name, "tokyo-night");

        let profiles = manager.list_profiles();
        assert!(profiles.contains(&"test".to_string()));
        assert!(profiles.contains(&"default".to_string()));
    }

    #[test]
    fn test_config_path() {
        let path = get_config_path();
        assert!(path.ends_with("tui-config.json"));
    }

    #[test]
    fn test_save_and_load_config() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("test-config.json");

        let config = TUIConfig {
            theme: ThemeConfig {
                name: "dracula".to_string(),
                custom_colors: None,
            },
            ..Default::default()
        };

        let content = serde_json::to_string_pretty(&config).unwrap();
        std::fs::write(&config_path, &content).unwrap();

        let loaded: TUIConfig = serde_json::from_str(&content).unwrap();
        assert_eq!(loaded.theme.name, "dracula");
    }

    #[test]
    fn test_behavior_config_mouse_scroll_speed_default() {
        let config = BehaviorConfig {
            auto_save_interval_seconds: 30,
            max_history_size: 1000,
            confirm_on_dangerous: true,
            yolo_mode: false,
            auto_scroll: true,
            stream_responses: true,
            mouse_scroll_speed: 0,
            vim_enabled: false,
            reduced_motion: false,
        };

        assert_eq!(config.mouse_scroll_speed(), 1);
    }

    #[test]
    fn test_behavior_config_default_scroll_speed() {
        let config = TUIConfig::default();
        assert_eq!(config.behavior.mouse_scroll_speed, 3);
        assert_eq!(config.behavior.mouse_scroll_speed(), 3);
    }

    #[test]
    fn test_behavior_config_scroll_speed_bounds_low() {
        let clamped = BehaviorConfig::validate_scroll_speed(0);
        assert_eq!(clamped, 1);

        let clamped = BehaviorConfig::validate_scroll_speed(1);
        assert_eq!(clamped, 1);
    }

    #[test]
    fn test_behavior_config_scroll_speed_bounds_high() {
        let clamped = BehaviorConfig::validate_scroll_speed(10);
        assert_eq!(clamped, 10);

        let clamped = BehaviorConfig::validate_scroll_speed(15);
        assert_eq!(clamped, 10);

        let clamped = BehaviorConfig::validate_scroll_speed(255);
        assert_eq!(clamped, 10);
    }

    #[test]
    fn test_behavior_config_set_mouse_scroll_speed() {
        let mut config = BehaviorConfig {
            auto_save_interval_seconds: 30,
            max_history_size: 1000,
            confirm_on_dangerous: true,
            yolo_mode: false,
            auto_scroll: true,
            stream_responses: true,
            mouse_scroll_speed: 3,
            vim_enabled: false,
            reduced_motion: false,
        };

        config.set_mouse_scroll_speed(5);
        assert_eq!(config.mouse_scroll_speed, 5);

        config.set_mouse_scroll_speed(0);
        assert_eq!(config.mouse_scroll_speed, 1);

        config.set_mouse_scroll_speed(20);
        assert_eq!(config.mouse_scroll_speed, 10);
    }

    #[test]
    fn test_behavior_config_vim_enabled_default() {
        let config = BehaviorConfig {
            auto_save_interval_seconds: 30,
            max_history_size: 1000,
            confirm_on_dangerous: true,
            yolo_mode: false,
            auto_scroll: true,
            stream_responses: true,
            mouse_scroll_speed: 3,
            vim_enabled: false,
            reduced_motion: false,
        };

        assert!(!config.vim_enabled);
    }

    #[test]
    fn test_tui_config_default_vim_disabled() {
        let config = TUIConfig::default();
        assert!(!config.behavior.vim_enabled);
    }

    #[test]
    fn test_tui_config_default_renderer() {
        let config = TUIConfig::default();
        assert_eq!(config.renderer.min_height, 8);
        assert_eq!(config.renderer.collapse_chrome_below_height, 12);
        assert_eq!(config.renderer.confirmation_width_percent, 60);
        assert_eq!(config.renderer.chrome_background, "#171717");
    }
}
