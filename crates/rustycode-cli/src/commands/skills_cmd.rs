//! Skills command implementation for skill management
//!
//! Provides CLI commands for:
//! - Listing available skills (built-in and custom)
//! - Running skills with variable substitution
//! - Managing custom skill definitions

use super::cli_args::SkillsCommand;
use anyhow::{Context, Result};
use rustycode_config::paths::RustyCodePath;
use rustycode_tools::skills::{Skill, SkillRegistry};
use std::collections::HashMap;
use std::path::PathBuf;

/// Execute skills command
pub async fn execute(cmd: SkillsCommand, format: &str) -> Result<()> {
    match cmd {
        SkillsCommand::List { detailed } => {
            let registry = load_skill_registry()?;
            let skills = registry.list();

            if skills.is_empty() {
                println!("No skills found.");
                println!("\nBuilt-in skills will be available once you run them.");
                return Ok(());
            }

            if format == "json" {
                println!("{}", serde_json::to_string_pretty(&skills)?);
            } else {
                println!("Available skills ({}):\n", skills.len());

                for skill in skills {
                    print_skill(skill, detailed);
                }
            }
        }
        SkillsCommand::Run {
            name,
            vars,
            dry_run,
        } => {
            let mut registry = load_skill_registry()?;
            let variables = parse_variables(&vars)?;

            // Check if skill exists
            if registry.get(&name).is_none() {
                // Check if it's a built-in skill that hasn't been registered yet
                let builtin_exists = SkillRegistry::builtin_skills()
                    .iter()
                    .any(|s| s.name == name);

                if builtin_exists {
                    println!("Note: Registering built-in skill '{}'", name);
                    for skill in SkillRegistry::builtin_skills() {
                        registry.register(skill);
                    }
                } else {
                    anyhow::bail!(
                        "Skill '{}' not found. Use 'skills list' to see available skills.",
                        name
                    );
                }
            }

            let resolved = registry.resolve(&name, &variables)?;

            println!("\n{}", resolved.rendered_prompt);

            if dry_run {
                println!("\n[Dry run mode - not executing]");
            } else {
                // Execute the skill prompt through the LLM
                use rustycode_llm::{create_provider_with_config, load_provider_config_from_env};
                use rustycode_orchestration::config::OrchestrationConfig;
                use rustycode_orchestration::pipeline::OrchestrationPipeline;

                let (provider_type, model_name, v2_config) = load_provider_config_from_env()
                    .context("Failed to load LLM provider config")?;
                let provider = create_provider_with_config(&provider_type, &model_name, v2_config)
                    .context("Failed to create LLM provider")?;

                let config = OrchestrationConfig::default();
                let model = resolved.skill.model.as_deref().unwrap_or(&model_name);
                let pipeline =
                    OrchestrationPipeline::with_provider_and_model(config, provider, model);

                let result = pipeline
                    .conduct(
                        format!("skill-{}", resolved.skill.name),
                        resolved.rendered_prompt.clone(),
                    )
                    .await?;

                match result {
                    rustycode_orchestration::pipeline::TaskResult::Success { output, .. } => {
                        println!("\n{}", output);
                    }
                    rustycode_orchestration::pipeline::TaskResult::Failed { reason, .. } => {
                        anyhow::bail!("Skill execution failed: {}", reason);
                    }
                }
            }

            // Print skill metadata
            if let Some(model) = &resolved.skill.model {
                println!("\nModel override: {}", model);
            }
            if let Some(temp) = resolved.skill.temperature {
                println!("Temperature override: {:.1}", temp);
            }
            if !resolved.skill.tools.is_empty() {
                println!("Tools: {}", resolved.skill.tools.join(", "));
            }
        }
        SkillsCommand::Create {
            name,
            description,
            prompt,
            variables,
            output,
        } => {
            let variables = if let Some(vars_str) = variables {
                parse_variable_definitions(&vars_str)?
            } else {
                Vec::new()
            };

            let skill = Skill {
                name: name.clone(),
                description,
                prompt,
                variables,
                tools: Vec::new(),
                model: None,
                temperature: None,
            };

            let output_path = if let Some(path) = output {
                PathBuf::from(path)
            } else {
                PathBuf::from(".")
            };

            let yaml_content = serde_yaml::to_string(&skill)?;
            std::fs::write(&output_path, yaml_content)?;

            println!("Created skill '{}' at {}", name, output_path.display());
            println!("\nYou can now run it with:");
            println!("  rustycode skills run {}", name);
        }
        SkillsCommand::Validate { path } => {
            let path_buf = PathBuf::from(&path);
            let content = std::fs::read_to_string(&path_buf)
                .map_err(|e| anyhow::anyhow!("Failed to read skill file at {}: {}", path, e))?;

            let extension = path_buf
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("yaml");

            let skill: Skill = match extension {
                "yaml" | "yml" => serde_yaml::from_str(&content)?,
                "toml" => toml::from_str(&content)?,
                "json" => serde_json::from_str(&content)?,
                _ => anyhow::bail!("Unsupported file format: {}", extension),
            };

            println!("Skill definition is valid!\n");
            print_skill(&skill, true);

            // Validate prompt template
            let placeholder_pattern = regex::Regex::new(r"\{\{(\w+)\}\}")
                .expect("placeholder regex is a valid constant pattern");
            let placeholders: Vec<_> = placeholder_pattern
                .find_iter(&skill.prompt)
                .map(|m| m.as_str().to_string())
                .collect();

            if !placeholders.is_empty() {
                println!("\nPrompt placeholders found:");
                for placeholder in &placeholders {
                    let var_name = placeholder.trim_start_matches("{{").trim_end_matches("}}");
                    let is_defined = skill.variables.iter().any(|v| v.name == var_name);
                    let status = if is_defined { "✓" } else { "✗" };
                    println!("  {} {}", status, placeholder);

                    if !is_defined {
                        println!(
                            "    Warning: Variable '{}' is used in prompt but not defined",
                            var_name
                        );
                    }
                }
            }
        }
    }

    Ok(())
}

/// Load skill registry with built-in and custom skills
fn load_skill_registry() -> Result<SkillRegistry> {
    let mut registry = SkillRegistry::new();

    // Register built-in skills
    for skill in SkillRegistry::builtin_skills() {
        registry.register(skill);
    }

    // Load custom skills from ~/.rustycode/skills
    if let Ok(skills_dir) = RustyCodePath::skills_dir() {
        if skills_dir.exists() {
            let custom_registry = SkillRegistry::load_from_dir(&skills_dir)?;
            for skill in custom_registry.list() {
                let skill = skill.clone();
                registry.register(skill);
            }
        }
    }

    Ok(registry)
}

/// Parse key=value pairs into HashMap
fn parse_variables(vars: &[String]) -> Result<HashMap<String, String>> {
    let mut result = HashMap::new();

    for var in vars {
        let mut parts = var.splitn(2, '=');
        let key = parts
            .next()
            .ok_or_else(|| anyhow::anyhow!("Missing key in variable assignment"))?;
        let value = parts
            .next()
            .ok_or_else(|| anyhow::anyhow!("Missing value for key '{}'", key))?;

        result.insert(key.to_string(), value.to_string());
    }

    Ok(result)
}

/// Parse variable definitions from "name:description:required" format
fn parse_variable_definitions(
    vars_str: &str,
) -> Result<Vec<rustycode_tools::skills::SkillVariable>> {
    let mut variables = Vec::new();

    for var_def in vars_str.split(',') {
        let parts: Vec<&str> = var_def.split(':').collect();
        if parts.is_empty() || parts[0].is_empty() {
            continue;
        }

        let name = parts[0].trim().to_string();
        let description = if parts.len() > 1 {
            parts[1].trim().to_string()
        } else {
            String::new()
        };
        let required = if parts.len() > 2 {
            parts[2].trim().eq_ignore_ascii_case("true") || parts[2].trim() == "1"
        } else {
            false
        };

        variables.push(rustycode_tools::skills::SkillVariable {
            name,
            description,
            required,
            default: None,
        });
    }

    Ok(variables)
}

/// Print skill information
fn print_skill(skill: &Skill, detailed: bool) {
    println!("  {} — {}", skill.name, skill.description);

    if detailed {
        if !skill.variables.is_empty() {
            println!("    Variables:");
            for var in &skill.variables {
                let required_marker = if var.required { "*" } else { "" };
                println!(
                    "      - {}{}: {}",
                    var.name, required_marker, var.description
                );
                if let Some(default) = &var.default {
                    println!("        (default: {})", default);
                }
            }
        }

        if !skill.tools.is_empty() {
            println!("    Tools: {}", skill.tools.join(", "));
        }

        if let Some(model) = &skill.model {
            println!("    Model: {}", model);
        }

        if let Some(temp) = skill.temperature {
            println!("    Temperature: {:.1}", temp);
        }

        println!();
    }
}
