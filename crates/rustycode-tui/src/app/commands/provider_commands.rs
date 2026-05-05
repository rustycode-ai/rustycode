//! Provider and model configuration commands

use super::CommandContext;
use super::CommandEffect;
use anyhow::Result;

/// Handle /model command
pub fn handle_model_command(parts: &[&str], _ctx: CommandContext<'_>) -> Result<CommandEffect> {
    use crate::providers::all_available_models;

    if parts.len() < 2 {
        let override_model = std::env::var("RUSTYCODE_MODEL_OVERRIDE").ok();
        let base_model =
            rustycode_llm::load_model_from_config().unwrap_or_else(|_| "unknown".to_string());

        let msg = if let Some(ovr) = override_model {
            format!(
                "Current model override: {}\nBase config model: {}",
                ovr, base_model
            )
        } else {
            format!(
                "Current model: {}\nSet a runtime override with `/model <number>` or `/model <model_id>`.\nUse `/model list` to see available models.",
                base_model
            )
        };

        return Ok(CommandEffect::SystemMessage(msg));
    }

    // Handle /model list subcommand
    if parts[1] == "list" {
        let models = all_available_models();
        if models.is_empty() {
            return Ok(CommandEffect::SystemMessage(
                "No models available. Configure a provider first with /provider.".to_string(),
            ));
        }
        let mut msg = "Available models:\n\n".to_string();
        for (i, model) in models.iter().enumerate() {
            msg.push_str(&format!("{}. {} ({})\n", i + 1, model.name, model.provider));
        }
        msg.push_str("\nSwitch with: /model <number> or /model <model_id>");
        return Ok(CommandEffect::SystemMessage(msg));
    }

    if let Ok(num) = parts[1].parse::<usize>() {
        let models = all_available_models();
        if let Some(model) = models.get(num - 1) {
            std::env::set_var("RUSTYCODE_MODEL_OVERRIDE", &model.id);
            return Ok(CommandEffect::ModelSwitch {
                model_id: model.id.clone(),
            });
        }

        return Ok(CommandEffect::SystemMessage(format!(
            "✗ Invalid model number: {}. Use /model to see available models, or press F5.",
            num
        )));
    }

    let new_model = parts[1].to_string();

    // Validate against available models
    let available = all_available_models();
    let is_known = available.iter().any(|m| m.id == new_model);
    if !is_known {
        let suggestions: Vec<String> = available
            .iter()
            .filter(|m| m.id.contains(&new_model) || new_model.contains(&m.id))
            .map(|m| format!("  {} ({})", m.name, m.id))
            .take(5)
            .collect();
        let hint = if suggestions.is_empty() {
            String::new()
        } else {
            format!("\n\nDid you mean one of these?\n{}", suggestions.join("\n"))
        };
        return Ok(CommandEffect::SystemMessage(format!(
            "⚠️  Model `{}` is not in the known model list. It may not work.\n\nUse /model list to see available models, or press F5 to open the model selector.{}",
            new_model, hint
        )));
    }

    std::env::set_var("RUSTYCODE_MODEL_OVERRIDE", &new_model);

    // Auto-detect provider from model ID if it's a cross-provider entry
    let provider_hint = if new_model.starts_with("anthropic/")
        || new_model.starts_with("openai/")
        || new_model.starts_with("google/")
        || new_model.starts_with("meta-llama/")
        || new_model.starts_with("microsoft/")
        || new_model.starts_with("deepseek/")
    {
        "openrouter"
    } else if new_model.contains("copilot") {
        "copilot"
    } else if new_model.contains("gemma-4")
        || new_model.contains("gemma-3n")
        || new_model.contains("gemma3-")
        || new_model.contains("phi-4-mini")
        || new_model.contains("qwen2.5")
        || new_model.contains("functiongemma")
    {
        "litert-lm"
    } else {
        ""
    };
    if !provider_hint.is_empty() {
        std::env::set_var("RUSTYCODE_PROVIDER_OVERRIDE", provider_hint);
    }

    Ok(CommandEffect::ModelSwitch {
        model_id: new_model,
    })
}

/// Handle /provider command
pub fn handle_provider_command(parts: &[&str], ctx: CommandContext<'_>) -> Result<CommandEffect> {
    use crate::providers::available_providers;

    let cwd = ctx.cwd;

    if parts.len() < 2 {
        let current = std::env::var("RUSTYCODE_PROVIDER")
            .ok()
            .unwrap_or_else(|| "default".to_string());
        let providers = available_providers();

        let mut lines = vec![
            format!("Current provider: {}", current),
            "".to_string(),
            "Available providers:".to_string(),
        ];
        for (i, p) in providers.iter().enumerate() {
            let status = if p.is_configured() { "✓" } else { "✗" };
            lines.push(format!("{}. {} [{}]", i + 1, p.name, status));
        }
        lines.push("".to_string());
        lines.push("Commands:".to_string());
        lines.push("  /provider <number>           - Switch provider".to_string());
        lines.push("  /provider connect <number>   - Configure API key".to_string());
        lines.push("  /provider disconnect <number>- Remove config".to_string());
        lines.push("  /provider validate <number>  - Test credentials".to_string());

        return Ok(CommandEffect::MultipleMessages(lines));
    }

    let subcommand = parts[1].to_string();

    // /provider list — same as bare /provider (show providers)
    if subcommand == "list" {
        let current = std::env::var("RUSTYCODE_PROVIDER")
            .ok()
            .unwrap_or_else(|| "default".to_string());
        let providers = available_providers();
        let mut lines = vec![
            format!("Current provider: {}", current),
            "".to_string(),
            "Available providers:".to_string(),
        ];
        for (i, p) in providers.iter().enumerate() {
            let status = if p.is_configured() { "✓" } else { "✗" };
            lines.push(format!("{}. {} [{}]", i + 1, p.name, status));
        }
        return Ok(CommandEffect::MultipleMessages(lines));
    }

    if subcommand == "connect" || subcommand == "disconnect" || subcommand == "validate" {
        if parts.len() < 3 {
            return Ok(CommandEffect::SystemMessage(format!(
                "Usage: /provider {} <number>\nExample: /provider {} 1",
                subcommand, subcommand
            )));
        }

        let num = match parts[2].parse::<usize>() {
            Ok(n) => n,
            Err(_) => {
                return Ok(CommandEffect::SystemMessage(format!(
                    "Invalid number: {}",
                    parts[2]
                )));
            }
        };

        let providers = available_providers();
        if num == 0 || num > providers.len() {
            return Ok(CommandEffect::SystemMessage(format!(
                "Invalid provider number {}. Valid range: 1-{}",
                num,
                providers.len()
            )));
        }
        if let Some(provider) = providers.get(num - 1) {
            return match subcommand.as_str() {
                "connect" => {
                    let env_var = format!(
                        "{}_API_KEY",
                        provider.provider_type.to_uppercase().replace('-', "_")
                    );
                    Ok(CommandEffect::SystemMessage(format!(
                        "To configure {}, set the API key:\n\nMethod 1 (Environment Variable):\n  export {}=\"your-api-key\"\n\nMethod 2 (Config File):\n  Edit ~/.rustycode/config.json:\n  {{\n    \"providers\": {{\n      \"{}\": {{\n        \"api_key\": \"your-api-key\"\n      }}\n    }}\n  }}\n\nThen restart the TUI.",
                        provider.name, env_var, provider.provider_type
                    )))
                }
                "disconnect" => {
                    let result = disconnect_provider(cwd, &provider.provider_type);
                    match result {
                        Ok(()) => Ok(CommandEffect::SystemMessage(format!(
                            "✓ Disconnected {}. Restart the TUI for changes to take effect.",
                            provider.name
                        ))),
                        Err(e) => Ok(CommandEffect::SystemMessage(format!(
                            "✗ Failed to disconnect {}: {}",
                            provider.name, e
                        ))),
                    }
                }
                "validate" => {
                    let result = validate_provider(cwd, &provider.provider_type);
                    Ok(CommandEffect::SystemMessage(format!(
                        "Testing {} credentials...\n\n{}",
                        provider.name, result
                    )))
                }
                _ => Ok(CommandEffect::SystemMessage(format!(
                    "Unknown command: {}. Available: connect, disconnect, validate",
                    subcommand
                ))),
            };
        }

        return Ok(CommandEffect::SystemMessage(format!(
            "Invalid provider number: {}",
            num
        )));
    }

    if let Ok(num) = parts[1].parse::<usize>() {
        let providers = available_providers();
        if let Some(provider) = providers.get(num - 1) {
            std::env::set_var("RUSTYCODE_PROVIDER", &provider.provider_type);
            return Ok(CommandEffect::SystemMessage(format!(
                "✓ Provider set to `{}`. Restart or wait for next request to take effect.",
                provider.name
            )));
        }

        return Ok(CommandEffect::SystemMessage(format!(
            "Invalid provider number: {}. Use /provider to see available providers.",
            num
        )));
    }

    let new_provider = parts[1].to_string();
    std::env::set_var("RUSTYCODE_PROVIDER", &new_provider);
    Ok(CommandEffect::SystemMessage(format!(
        "✓ Provider override set to `{}`. Restart or wait for next request to take effect.",
        new_provider
    )))
}

fn disconnect_provider(
    cwd: &std::path::Path,
    provider_type: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use rustycode_config::Config;

    let config_path = crate::app::wizard_handler::WizardHandler::config_path(cwd);
    if !config_path.exists() {
        return Err("No config file found".into());
    }

    let mut config = Config::load(cwd)?;
    match provider_type {
        "anthropic" => config.providers.anthropic = None,
        "openai" => config.providers.openai = None,
        "openrouter" => config.providers.openrouter = None,
        _ => {
            config.providers.custom.remove(provider_type);
        }
    }
    config.save(&config_path)?;
    Ok(())
}

fn validate_provider(cwd: &std::path::Path, provider_type: &str) -> String {
    use rustycode_config::Config;

    match Config::load(cwd) {
        Ok(config) => {
            let has_config = match provider_type {
                "anthropic" => config.providers.anthropic.is_some(),
                "openai" => config.providers.openai.is_some(),
                "openrouter" => config.providers.openrouter.is_some(),
                _ => config.providers.custom.contains_key(provider_type),
            };

            if has_config {
                format!(
                    "✓ {} is configured.\n\nNote: To fully test credentials, try sending a message.\nIf there's an issue with your API key, you'll see an error when making a request.",
                    provider_type
                )
            } else {
                format!(
                    "✗ {} is not configured.\n\nUse /provider connect {} to set up your API key.",
                    provider_type, provider_type
                )
            }
        }
        Err(e) => format!(
            "⚠ Could not load config: {}\n\nPlease check your configuration file.",
            e
        ),
    }
}
