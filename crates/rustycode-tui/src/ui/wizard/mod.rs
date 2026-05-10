//! First-Run Configuration Wizard
//!
//! This module provides a user-friendly wizard that runs on first launch to help users:
//! - Configure their AI provider (Anthropic, OpenAI, etc.)
//! - Select their preferred model
//! - Set up API keys
//! - Configure basic settings

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rustycode_config::{Config, ProviderConfig};
use std::path::PathBuf;

/// Wizard state machine
#[derive(Debug, Clone, PartialEq)]
pub enum WizardStep {
    Welcome,
    SelectProvider,
    /// GitHub Copilot device flow: shows user_code and polling status
    CopilotDeviceFlow,
    ConfigureProvider,
    SelectModel,
    Review,
    Complete,
}

/// Provider information for the wizard
#[derive(Debug, Clone)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub requires_api_key: bool,
    pub default_models: Vec<String>,
    pub popular: bool,
}

/// First-run wizard state
pub struct FirstRunWizard {
    pub step: WizardStep,
    pub providers: Vec<ProviderInfo>,
    pub selected_provider_index: usize,
    pub api_key_input: String,
    pub selected_model_index: usize,
    pub config: Config,
    pub config_path: PathBuf,
    pub error_message: Option<String>,
    pub show_help: bool,
    /// Copilot device flow: the verification URL to show the user
    pub copilot_verification_uri: String,
    /// Copilot device flow: the user code to display
    pub copilot_user_code: String,
    /// Copilot device flow: status message (e.g. "Waiting for authorization...")
    pub copilot_status: String,
    /// Copilot device flow: the obtained copilot token (set when complete)
    pub copilot_token: Option<String>,
}

impl FirstRunWizard {
    pub fn new(config_path: PathBuf) -> Self {
        let providers = Self::available_providers();
        let config = Config::default();

        Self {
            step: WizardStep::Welcome,
            providers,
            selected_provider_index: 0,
            api_key_input: String::new(),
            selected_model_index: 0,
            config,
            config_path,
            error_message: None,
            show_help: false,
            copilot_verification_uri: String::new(),
            copilot_user_code: String::new(),
            copilot_status: String::new(),
            copilot_token: None,
        }
    }

    /// Get available providers for the wizard
    fn available_providers() -> Vec<ProviderInfo> {
        vec![
            ProviderInfo {
                id: "anthropic".to_string(),
                name: "Anthropic Claude".to_string(),
                description: "Most capable AI assistant for complex tasks".to_string(),
                requires_api_key: true,
                default_models: vec![
                    "claude-sonnet-4-6".to_string(),
                    "claude-3-5-haiku-20241022".to_string(),
                    "claude-3-opus-20240229".to_string(),
                ],
                popular: true,
            },
            ProviderInfo {
                id: "openai".to_string(),
                name: "OpenAI GPT".to_string(),
                description: "Fast and capable, great for coding tasks".to_string(),
                requires_api_key: true,
                default_models: vec![
                    "gpt-4o".to_string(),
                    "gpt-4o-mini".to_string(),
                    "gpt-4-turbo".to_string(),
                    "gpt-3.5-turbo".to_string(),
                ],
                popular: true,
            },
            ProviderInfo {
                id: "copilot".to_string(),
                name: "GitHub Copilot".to_string(),
                description: "GitHub Copilot — sign in with your GitHub account (device flow)"
                    .to_string(),
                requires_api_key: false,
                default_models: vec![
                    "gpt-4.1-copilot".to_string(),
                    "gpt-4o-copilot".to_string(),
                    "gpt-4o-mini-copilot".to_string(),
                    "o3-mini-copilot".to_string(),
                ],
                popular: true,
            },
            ProviderInfo {
                id: "kimi-global".to_string(),
                name: "Kimi (Global)".to_string(),
                description: "Moonshot AI's Kimi models - Global endpoint".to_string(),
                requires_api_key: true,
                default_models: vec!["kimi-k2".to_string(), "kimi-latest".to_string()],
                popular: false,
            },
            ProviderInfo {
                id: "kimi-cn".to_string(),
                name: "Kimi (China)".to_string(),
                description: "Moonshot AI's Kimi models - China endpoint".to_string(),
                requires_api_key: true,
                default_models: vec!["kimi-k2".to_string(), "kimi-latest".to_string()],
                popular: false,
            },
            ProviderInfo {
                id: "alibaba-global".to_string(),
                name: "Alibaba Qwen (Global)".to_string(),
                description: "Alibaba's Qwen models via DashScope - Global endpoint".to_string(),
                requires_api_key: true,
                default_models: vec!["qwen-max".to_string(), "qwen-coder-plus".to_string()],
                popular: false,
            },
            ProviderInfo {
                id: "alibaba-cn".to_string(),
                name: "Alibaba Qwen (China)".to_string(),
                description: "Alibaba's Qwen models via DashScope - China endpoint".to_string(),
                requires_api_key: true,
                default_models: vec!["qwen-max".to_string(), "qwen-coder-plus".to_string()],
                popular: false,
            },
            ProviderInfo {
                id: "vertex".to_string(),
                name: "Google Vertex AI".to_string(),
                description: "Google's Gemini models via Vertex AI platform".to_string(),
                requires_api_key: true,
                default_models: vec!["gemini-1.5-pro".to_string(), "gemini-1.5-flash".to_string()],
                popular: false,
            },
            ProviderInfo {
                id: "openrouter".to_string(),
                name: "OpenRouter".to_string(),
                description: "Access to multiple models through one API".to_string(),
                requires_api_key: true,
                default_models: vec![
                    "anthropic/claude-3.5-sonnet".to_string(),
                    "openai/gpt-4o".to_string(),
                    "google/gemini-pro-1.5".to_string(),
                ],
                popular: false,
            },
            ProviderInfo {
                id: "ollama".to_string(),
                name: "Ollama".to_string(),
                description: "Run models locally on your machine".to_string(),
                requires_api_key: false,
                default_models: vec![
                    "llama3.1".to_string(),
                    "mistral".to_string(),
                    "codellama".to_string(),
                ],
                popular: false,
            },
        ]
    }

    pub fn selected_provider(&self) -> &ProviderInfo {
        let idx = self
            .selected_provider_index
            .min(self.providers.len().saturating_sub(1));
        &self.providers[idx]
    }

    /// Get available models for the selected provider
    pub fn available_models(&self) -> Vec<String> {
        self.selected_provider().default_models.clone()
    }

    pub fn selected_model(&self) -> String {
        let models = self.available_models();
        if self.selected_model_index < models.len() {
            models[self.selected_model_index].clone()
        } else {
            models.first().cloned().unwrap_or_default()
        }
    }

    /// Handle key events in the wizard
    pub fn handle_key_event(&mut self, key: KeyEvent) -> WizardAction {
        // Handle Ctrl+C globally to quit wizard
        if key.code == KeyCode::Char('c') && key.modifiers == KeyModifiers::CONTROL {
            return WizardAction::Quit;
        }

        match self.step {
            WizardStep::Welcome => self.handle_welcome_key(key),
            WizardStep::SelectProvider => self.handle_provider_selection_key(key),
            WizardStep::CopilotDeviceFlow => self.handle_copilot_device_flow_key(key),
            WizardStep::ConfigureProvider => self.handle_provider_config_key(key),
            WizardStep::SelectModel => self.handle_model_selection_key(key),
            WizardStep::Review => self.handle_review_key(key),
            WizardStep::Complete => self.handle_complete_key(key),
        }
    }

    /// Handle keys in welcome step
    fn handle_welcome_key(&mut self, key: KeyEvent) -> WizardAction {
        match key.code {
            KeyCode::Enter | KeyCode::Char(' ') => {
                self.step = WizardStep::SelectProvider;
                WizardAction::Continue
            }
            KeyCode::Char('q') | KeyCode::Esc => WizardAction::Quit,
            KeyCode::Char('?') => {
                self.show_help = !self.show_help;
                WizardAction::Continue
            }
            _ => WizardAction::Continue,
        }
    }

    /// Handle keys in provider selection step
    fn handle_provider_selection_key(&mut self, key: KeyEvent) -> WizardAction {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected_provider_index > 0 {
                    self.selected_provider_index -= 1;
                }
                WizardAction::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.selected_provider_index < self.providers.len() - 1 {
                    self.selected_provider_index += 1;
                }
                WizardAction::Continue
            }
            KeyCode::Enter => {
                let provider = self.selected_provider();
                if provider.id == "copilot" {
                    // Start the GitHub Copilot device flow
                    self.step = WizardStep::CopilotDeviceFlow;
                    self.copilot_status = "Starting device flow...".into();
                    // Kick off async device flow in background via a blocking thread
                    self.start_copilot_device_flow();
                } else {
                    self.step = WizardStep::ConfigureProvider;
                }
                WizardAction::Continue
            }
            KeyCode::Esc => {
                self.step = WizardStep::Welcome;
                WizardAction::Continue
            }
            _ => WizardAction::Continue,
        }
    }

    /// Handle keys in provider configuration step
    fn handle_provider_config_key(&mut self, key: KeyEvent) -> WizardAction {
        match key.code {
            KeyCode::Char(c) if c.is_ascii() => {
                self.api_key_input.push(c);
                WizardAction::Continue
            }
            KeyCode::Backspace => {
                self.api_key_input.pop();
                WizardAction::Continue
            }
            KeyCode::Enter => {
                if self.validate_api_key() {
                    self.step = WizardStep::SelectModel;
                    self.error_message = None;
                } else {
                    self.error_message = Some("Please enter a valid API key".to_string());
                }
                WizardAction::Continue
            }
            KeyCode::Esc => {
                self.step = WizardStep::SelectProvider;
                self.error_message = None;
                WizardAction::Continue
            }
            _ => WizardAction::Continue,
        }
    }

    /// Handle keys in model selection step
    fn handle_model_selection_key(&mut self, key: KeyEvent) -> WizardAction {
        let models = self.available_models();

        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected_model_index > 0 {
                    self.selected_model_index -= 1;
                }
                WizardAction::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.selected_model_index < models.len().saturating_sub(1) {
                    self.selected_model_index += 1;
                }
                WizardAction::Continue
            }
            KeyCode::Enter => {
                self.step = WizardStep::Review;
                self.update_config_from_selection();
                WizardAction::Continue
            }
            KeyCode::Esc => {
                self.step = WizardStep::ConfigureProvider;
                WizardAction::Continue
            }
            _ => WizardAction::Continue,
        }
    }

    /// Handle keys in review step
    fn handle_review_key(&mut self, key: KeyEvent) -> WizardAction {
        match key.code {
            KeyCode::Enter | KeyCode::Char(' ') => match self.save_config() {
                Ok(()) => {
                    self.step = WizardStep::Complete;
                    WizardAction::Continue
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to save config: {}", e));
                    WizardAction::Continue
                }
            },
            KeyCode::Esc => {
                self.step = WizardStep::SelectModel;
                WizardAction::Continue
            }
            KeyCode::Char('r') => {
                self.step = WizardStep::SelectProvider;
                WizardAction::Continue
            }
            _ => WizardAction::Continue,
        }
    }

    /// Handle keys in complete step
    fn handle_complete_key(&mut self, key: KeyEvent) -> WizardAction {
        match key.code {
            KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Esc | KeyCode::Char('q') => {
                WizardAction::Finish
            }
            _ => WizardAction::Continue,
        }
    }

    /// Start the GitHub Copilot device flow in a background thread.
    fn start_copilot_device_flow(&mut self) {
        // Set initial status
        self.copilot_status = "Requesting device code...".into();
        self.copilot_verification_uri.clear();
        self.copilot_user_code.clear();
        self.copilot_token = None;

        // Use a thread to run the blocking device flow since we can't easily
        // run async code from the synchronous TUI render loop.
        // We store results in files that we poll.
        let status_path = std::env::temp_dir().join("rustycode_copilot_status.json");
        // Remove any stale status file
        if let Err(e) = std::fs::remove_file(&status_path) {
            tracing::debug!("could not remove stale copilot status file: {e}");
        }

        std::thread::spawn(move || {
            use rustycode_auth::GitHubCopilotAuth;

            rustycode_shared_runtime::block_on_shared(async {
                let auth = GitHubCopilotAuth::new();

                // Step 1: Request device code
                let device = match auth.request_device_code().await {
                    Ok(d) => d,
                    Err(e) => {
                        if let Err(we) = std::fs::write(
                            &status_path,
                            serde_json::json!({"error": e.to_string()}).to_string(),
                        ) {
                            tracing::warn!("failed to write copilot error status: {we}");
                        }
                        return;
                    }
                };

                // Write the device code info so the TUI can display it
                if let Err(e) = std::fs::write(
                    &status_path,
                    serde_json::json!({
                        "stage": "waiting",
                        "user_code": device.user_code,
                        "verification_uri": device.verification_uri,
                    })
                    .to_string(),
                ) {
                    tracing::warn!("failed to write copilot device code status: {e}");
                }

                // Step 2: Poll for token
                let github_token = match auth
                    .poll_for_token(&device.device_code, device.interval, device.expires_in)
                    .await
                {
                    Ok(t) => t,
                    Err(e) => {
                        if let Err(we) = std::fs::write(
                            &status_path,
                            serde_json::json!({"error": e.to_string()}).to_string(),
                        ) {
                            tracing::warn!("failed to write copilot poll error status: {we}");
                        }
                        return;
                    }
                };

                // Update status
                let _ = std::fs::write(
                    &status_path,
                    serde_json::json!({"stage": "exchanging"}).to_string(),
                );

                // Step 3: Exchange for Copilot token
                match auth.exchange_for_copilot_token(&github_token).await {
                    Ok(result) => {
                        let _ = std::fs::write(
                            &status_path,
                            serde_json::json!({
                                "stage": "complete",
                                "copilot_token": result.copilot_token,
                                "expires_at": result.expires_at,
                            })
                            .to_string(),
                        );
                    }
                    Err(e) => {
                        let _ = std::fs::write(
                            &status_path,
                            serde_json::json!({"error": e.to_string()}).to_string(),
                        );
                    }
                }
            });
        });
    }

    /// Poll the status file written by the device flow thread.
    /// Returns true if the flow is complete.
    fn poll_copilot_status(&mut self) -> bool {
        let status_path = std::env::temp_dir().join("rustycode_copilot_status.json");
        let Ok(content) = std::fs::read_to_string(&status_path) else {
            return false;
        };
        let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) else {
            return false;
        };

        if let Some(error) = val.get("error").and_then(|v| v.as_str()) {
            self.copilot_status = format!("Error: {}", error);
            return false;
        }

        match val.get("stage").and_then(|v| v.as_str()) {
            Some("waiting") => {
                if let Some(code) = val.get("user_code").and_then(|v| v.as_str()) {
                    self.copilot_user_code = code.to_string();
                }
                if let Some(uri) = val.get("verification_uri").and_then(|v| v.as_str()) {
                    self.copilot_verification_uri = uri.to_string();
                }
                self.copilot_status = "Waiting for you to authorize in the browser...".into();
                false
            }
            Some("exchanging") => {
                self.copilot_status = "Authorization received, exchanging token...".into();
                false
            }
            Some("complete") => {
                if let Some(token) = val.get("copilot_token").and_then(|v| v.as_str()) {
                    self.copilot_token = Some(token.to_string());
                    self.copilot_status = "Login successful!".into();
                    // Clean up temp file
                    let _ = std::fs::remove_file(&status_path);
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    /// Handle keys in the Copilot device flow step
    fn handle_copilot_device_flow_key(&mut self, key: KeyEvent) -> WizardAction {
        // Poll for status updates
        let complete = self.poll_copilot_status();

        if complete {
            // Move to model selection
            self.step = WizardStep::SelectModel;
            self.error_message = None;
            return WizardAction::Continue;
        }

        match key.code {
            KeyCode::Esc => {
                // Cancel and go back
                let _ = std::fs::remove_file(
                    std::env::temp_dir().join("rustycode_copilot_status.json"),
                );
                self.step = WizardStep::SelectProvider;
                self.error_message = None;
                WizardAction::Continue
            }
            KeyCode::Char('r') => {
                // Retry: restart the device flow
                self.start_copilot_device_flow();
                WizardAction::Continue
            }
            _ => WizardAction::Continue,
        }
    }
}

impl FirstRunWizard {
    fn validate_api_key(&self) -> bool {
        let provider = self.selected_provider();

        if !provider.requires_api_key {
            return true; // Provider doesn't need API key
        }

        // Basic validation - check if key looks reasonable
        let key = self.api_key_input.trim();
        !key.is_empty() && key.len() >= 20
    }

    /// Update config from user selections
    pub fn update_config_from_selection(&mut self) {
        // Get the model value first
        let model_value = self.selected_model();

        // Get provider index and clone the provider info
        let provider_index = self.selected_provider_index;
        let provider_clone = self.providers[provider_index].clone();

        self.config.model = model_value;
        self.config.temperature = Some(0.1);
        self.config.max_tokens = Some(4096);

        // Configure the selected provider
        let api_key = if provider_clone.requires_api_key {
            Some(self.api_key_input.clone())
        } else {
            None
        };

        let provider_config = ProviderConfig {
            api_key,
            base_url: None,
            models: Some(provider_clone.default_models.clone()),
            headers: None,
        };

        // Update providers config based on selection
        match provider_clone.id.as_str() {
            "anthropic" => {
                self.config.providers.anthropic = Some(provider_config);
            }
            "openai" => {
                self.config.providers.openai = Some(provider_config);
            }
            "openrouter" => {
                self.config.providers.openrouter = Some(provider_config);
            }
            "copilot" => {
                // For Copilot, the token comes from the device flow
                let copilot_config = ProviderConfig {
                    api_key: self.copilot_token.clone(),
                    base_url: Some("https://api.githubcopilot.com".to_string()),
                    models: Some(provider_clone.default_models.clone()),
                    headers: None,
                };
                self.config.providers.custom.insert(
                    "copilot".to_string(),
                    serde_json::to_value(copilot_config).unwrap_or_default(),
                );
            }
            _ => {
                // For custom providers, add to the custom map
                self.config.providers.custom.insert(
                    provider_clone.id.clone(),
                    serde_json::to_value(provider_config).unwrap_or_default(),
                );
            }
        }
    }

    /// Save the configuration to file
    pub fn save_config(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Ensure the directory exists
        if let Some(parent) = self.config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Backup existing config if it exists
        if self.config_path.exists() {
            let backup_path = self.config_path.with_extension("json.bak");
            std::fs::copy(&self.config_path, &backup_path)?;
        }

        // Save the config
        self.config.save(&self.config_path)?;

        // Set secure file permissions (user read/write only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&self.config_path)?.permissions();
            perms.set_mode(0o600); // rw-------
            std::fs::set_permissions(&self.config_path, perms)?;
        }

        Ok(())
    }

    // Render methods are in render.rs
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WizardAction {
    /// Continue running the wizard
    Continue,
    /// Wizard is complete, exit
    Finish,
    /// User wants to quit
    Quit,
}

mod render;
#[cfg(test)]
mod tests;
