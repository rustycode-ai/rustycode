//! Model persistence for RustyCode
//!
//! This module handles saving and loading the last used model across sessions.

use anyhow::Result;
use std::path::PathBuf;

/// User preferences that persist across sessions
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UserPreferences {
    pub last_used_model: String,
    pub last_used_provider: String,
}

impl Default for UserPreferences {
    fn default() -> Self {
        Self {
            last_used_model: "claude-sonnet-4-6".to_string(),
            last_used_provider: "anthropic".to_string(),
        }
    }
}

impl UserPreferences {
    /// Get the path to the preferences file
    pub fn get_preferences_path() -> Result<PathBuf> {
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;

        let rustycode_dir = home.join(".rustycode");
        std::fs::create_dir_all(&rustycode_dir)
            .map_err(|e| anyhow::anyhow!("Failed to create .rustycode directory: {}", e))?;

        Ok(rustycode_dir.join("preferences.json"))
    }

    /// Load user preferences from disk
    pub fn load() -> Result<Self> {
        let path = Self::get_preferences_path()?;

        if !path.exists() {
            // Create default preferences if file doesn't exist
            let prefs = Self::default();
            prefs.save()?;
            return Ok(prefs);
        }

        let content = std::fs::read_to_string(&path)
            .map_err(|e| anyhow::anyhow!("Failed to read preferences: {}", e))?;

        let prefs: UserPreferences = serde_json::from_str(&content)
            .map_err(|e| anyhow::anyhow!("Failed to parse preferences: {}", e))?;

        Ok(prefs)
    }

    /// Save user preferences to disk
    pub fn save(&self) -> Result<()> {
        let path = Self::get_preferences_path()?;

        let content = serde_json::to_string_pretty(self)
            .map_err(|e| anyhow::anyhow!("Failed to serialize preferences: {}", e))?;

        std::fs::write(&path, content)
            .map_err(|e| anyhow::anyhow!("Failed to write preferences: {}", e))?;

        Ok(())
    }

    /// Update the last used model
    pub fn update_last_model(&mut self, model: String) -> Result<()> {
        self.last_used_model = model;
        self.save()
    }

    /// Update the last used provider
    pub fn update_last_provider(&mut self, provider: String) -> Result<()> {
        self.last_used_provider = provider;
        self.save()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_default_preferences() {
        let prefs = UserPreferences::default();
        assert_eq!(prefs.last_used_model, "claude-sonnet-4-6");
        assert_eq!(prefs.last_used_provider, "anthropic");
    }

    #[test]
    fn test_update_model() {
        let mut prefs = UserPreferences::default();
        prefs.update_last_model("gpt-4o".to_string()).unwrap();
        assert_eq!(prefs.last_used_model, "gpt-4o");
    }

    #[test]
    fn test_update_provider() {
        let mut prefs = UserPreferences::default();
        prefs.update_last_provider("openai".to_string()).unwrap();
        assert_eq!(prefs.last_used_provider, "openai");
    }

    #[test]
    fn test_save_and_load() {
        // Create a temporary directory for testing
        let temp_dir = tempfile::tempdir().unwrap();
        let test_file = temp_dir.path().join("test_preferences.json");

        // Create custom preferences
        let prefs = UserPreferences {
            last_used_model: "gpt-4o-mini".to_string(),
            last_used_provider: "openai".to_string(),
        };

        // Manually save to test file
        let content = serde_json::to_string_pretty(&prefs).unwrap();
        fs::write(&test_file, content).unwrap();

        // Read back and verify
        let loaded_content = fs::read_to_string(&test_file).unwrap();
        let loaded_prefs: UserPreferences = serde_json::from_str(&loaded_content).unwrap();

        assert_eq!(loaded_prefs.last_used_model, "gpt-4o-mini");
        assert_eq!(loaded_prefs.last_used_provider, "openai");
    }

    #[test]
    fn test_serialize_deserialize() {
        let prefs = UserPreferences {
            last_used_model: "claude-opus-4-6".to_string(),
            last_used_provider: "anthropic".to_string(),
        };

        // Serialize
        let json = serde_json::to_string(&prefs).unwrap();
        assert!(json.contains("claude-opus-4-6"));
        assert!(json.contains("anthropic"));

        // Deserialize
        let deserialized: UserPreferences = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.last_used_model, prefs.last_used_model);
        assert_eq!(deserialized.last_used_provider, prefs.last_used_provider);
    }
}
