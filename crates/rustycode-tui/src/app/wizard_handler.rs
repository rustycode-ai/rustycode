//! First-run wizard handler
//!
//! Handles wizard initialization, visibility, and completion for first-time setup.

use crate::ui::wizard::FirstRunWizard;
use std::path::{Path, PathBuf};

/// Wizard initialization and visibility management for the TUI
pub struct WizardHandler {
    /// The wizard component
    pub wizard: Option<FirstRunWizard>,
    /// Whether the wizard should be shown
    pub showing_wizard: bool,
}

impl WizardHandler {
    /// Create a new wizard handler
    pub fn new(cwd: &Path, reconfigure: bool) -> Self {
        let config_path = Self::get_config_path(cwd);
        let showing_wizard = Self::should_show_wizard(&config_path, reconfigure);

        Self {
            wizard: if showing_wizard {
                Some(FirstRunWizard::new(config_path))
            } else {
                None
            },
            showing_wizard,
        }
    }

    /// Check if the wizard should be shown (first-run detection)
    fn should_show_wizard(config_path: &Path, reconfigure: bool) -> bool {
        // Force show wizard if reconfigure flag is set
        if reconfigure {
            tracing::info!("Wizard: --reconfigure flag set");
            return true;
        }

        // Check if config file exists
        tracing::info!("Wizard: Checking config path: {:?}", config_path);

        if !config_path.exists() {
            tracing::info!("Wizard: Config file not found at {:?}", config_path);
            return true; // No config file, show wizard
        }

        tracing::info!("Wizard: Config file exists at {:?}", config_path);

        // Config exists but might be incomplete
        // Try to load it and check if providers are configured
        match rustycode_config::Config::load(config_path.parent().unwrap_or(Path::new("."))) {
            Ok(config) => {
                // Check if any provider is configured
                let has_anthropic = config.providers.anthropic.is_some();
                let has_openai = config.providers.openai.is_some();
                let has_openrouter = config.providers.openrouter.is_some();
                let has_custom = !config.providers.custom.is_empty();

                tracing::info!(
                    "Wizard: Provider status - anthropic: {}, openai: {}, openrouter: {}, custom: {}",
                    has_anthropic, has_openai, has_openrouter, has_custom
                );

                let has_configured_provider =
                    has_anthropic || has_openai || has_openrouter || has_custom;

                if has_configured_provider {
                    tracing::info!("Wizard: Provider configured, skipping wizard");
                    false // Don't show wizard
                } else {
                    tracing::info!("Wizard: Config exists but no provider configured");
                    true // Show wizard if no provider configured
                }
            }
            Err(e) => {
                tracing::warn!(
                    "Wizard: Failed to load config from {:?}: {:?}",
                    config_path,
                    e
                );
                true // Failed to load config, show wizard
            }
        }
    }

    /// Get the configuration file path (matches ConfigLoader search paths)
    pub(crate) fn get_config_path(cwd: &Path) -> PathBuf {
        // Check for local .rustycode/config.json first
        let local_config = cwd.join(".rustycode").join("config.json");
        if local_config.exists() {
            return local_config;
        }

        // Use the same XDG config directory as ConfigLoader
        if let Some(cfg_dir) = dirs::config_dir() {
            let xdg_config = cfg_dir.join("rustycode").join("config.json");
            if xdg_config.exists() {
                return xdg_config;
            }
        }

        // Fall back to legacy ~/.rustycode/config.json for backwards compatibility
        if let Ok(home) = std::env::var("HOME") {
            let legacy_config = PathBuf::from(home).join(".rustycode").join("config.json");
            if legacy_config.exists() {
                return legacy_config;
            }
        }

        // Default to XDG config path (where new configs should be created)
        dirs::config_dir()
            .map(|d| d.join("rustycode").join("config.json"))
            .unwrap_or_else(|| local_config)
    }

    /// Hide the wizard
    pub fn complete(&mut self) {
        self.showing_wizard = false;
    }
}
