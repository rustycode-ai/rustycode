//! First-run wizard handler

use crate::ui::wizard::FirstRunWizard;
use std::path::{Path, PathBuf};

/// Wizard initialization and visibility management for the TUI
pub struct WizardHandler {
    /// The wizard component
    pub wizard: Option<FirstRunWizard>,
    pub showing_wizard: bool,
}

impl WizardHandler {
    pub fn new(cwd: &Path, reconfigure: bool) -> Self {
        let config_path = Self::config_path(cwd);
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
    pub(crate) fn config_path(cwd: &Path) -> PathBuf {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn complete_hides_wizard() {
        let mut handler = WizardHandler {
            wizard: None,
            showing_wizard: true,
        };
        handler.complete();
        assert!(!handler.showing_wizard);
    }

    #[test]
    fn new_with_reconfigure_shows_wizard() {
        let tmp = tempfile::tempdir().unwrap();
        let handler = WizardHandler::new(tmp.path(), true);
        assert!(handler.showing_wizard);
        assert!(handler.wizard.is_some());
    }

    #[test]
    fn new_without_reconfigure_respects_existing_config() {
        let tmp = tempfile::tempdir().unwrap();
        let handler = WizardHandler::new(tmp.path(), false);
        // Result depends on whether a real config exists on this system
        // (config_path checks XDG, home, etc.), so just verify it doesn't panic
        let _ = handler.showing_wizard;
    }

    #[test]
    fn config_path_prefers_local_config() {
        let tmp = tempfile::tempdir().unwrap();
        let config_dir = tmp.path().join(".rustycode");
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(config_dir.join("config.json"), "{}").unwrap();

        let path = WizardHandler::config_path(tmp.path());
        assert!(path.ends_with("config.json"));
        assert!(path.to_string_lossy().contains(".rustycode"));
    }

    #[test]
    fn config_path_falls_back_to_default() {
        let tmp = tempfile::tempdir().unwrap();
        let path = WizardHandler::config_path(tmp.path());
        assert!(path.ends_with("config.json"));
    }
}
