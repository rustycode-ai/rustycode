//! Skills command implementation for skill management
//!
//! Provides CLI commands for:
//! - Listing available skills (built-in and custom)
//! - Running skills with variable substitution
//! - Managing custom skill definitions

use super::cli_args::SkillsCommand;
use anyhow::{Context, Result};
use rustycode_skill::manager::SkillManager;
use std::collections::HashMap;
use std::path::PathBuf;

/// Execute skills command
pub async fn execute(cmd: SkillsCommand, format: &str) -> Result<()> {
    match cmd {
        SkillsCommand::List { detailed } => {
            let mgr = load_skill_manager()?;
            let skills = mgr.all_definitions();

            if skills.is_empty() {
                println!("No skills found.");
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
            let mgr = load_skill_manager()?;

            // Skill name validation
            let skill = mgr.definition(&name).ok_or_else(|| {
                anyhow::anyhow!(
                    "Skill '{}' not found. Use 'skills list' to see available skills.",
                    name
                )
            })?;

            // Legacy variable substitution logic (kept for compatibility with 'run' command)
            let variables = parse_variables(&vars)?;
            let mut rendered_prompt = skill
                .content
                .as_deref()
                .unwrap_or(&skill.description)
                .to_string();

            for (key, value) in variables {
                rendered_prompt = rendered_prompt.replace(&format!("{{{{{key}}}}}"), &value);
            }

            println!("\n{}", rendered_prompt);

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
                let model = skill.model_override.as_deref().unwrap_or(&model_name);
                let pipeline =
                    OrchestrationPipeline::with_provider_and_model(config, provider, model);

                let result = pipeline
                    .conduct(format!("skill-{}", skill.name), rendered_prompt.clone())
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
            if let Some(model) = &skill.model_override {
                println!("\nModel override: {}", model);
            }
            if !skill.allowed_tools.is_empty() {
                println!("Allowed Tools: {}", skill.allowed_tools.join(", "));
            }
        }
        SkillsCommand::Create {
            name,
            description,
            prompt,
            variables: _,
            output,
        } => {
            // For the new SKILL.md format, we create a directory and a SKILL.md file
            let output_dir = if let Some(path) = output {
                PathBuf::from(path).join(&name)
            } else {
                PathBuf::from(".").join(&name)
            };

            std::fs::create_dir_all(&output_dir)?;

            let content = format!(
                "---\nname: {}\ndescription: {}\n---\n\n# {}\n\n{}",
                name, description, name, prompt
            );

            std::fs::write(output_dir.join("SKILL.md"), content)?;

            println!(
                "Created skill '{}' directory at {}",
                name,
                output_dir.display()
            );
            println!("\nYou can now run it with:");
            println!("  rustycode skills run {}", name);
        }
        SkillsCommand::Validate { path } => {
            let path_buf = PathBuf::from(&path);
            let skill_md = if path_buf.is_dir() {
                path_buf.join("SKILL.md")
            } else {
                path_buf
            };

            if !skill_md.exists() {
                anyhow::bail!("Skill file not found at {}", skill_md.display());
            }

            // Use the new registry's internal parsing logic for validation
            // We'll simulate this by creating a registry and loading the file
            let mut mgr = SkillManager::builder().build()?;
            // We need to access the registry internally or use load_from_dir
            if let Some(parent) = skill_md.parent() {
                mgr.discover_dynamic(&[], parent);
            }

            let name = skill_md
                .parent()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string());

            if let Some(skill) = mgr.definition(&name) {
                println!("Skill definition is valid!\n");
                print_skill(skill, true);
            } else {
                anyhow::bail!("Failed to validate skill at {}", skill_md.display());
            }
        }
    }

    Ok(())
}

/// Load skill manager with bundled and user skills
fn load_skill_manager() -> Result<SkillManager> {
    let mut builder = SkillManager::builder();

    // SkillManager builder automatically handles bundled skills and
    // we can specify user skills directory
    if let Ok(user_dir) = rustycode_config::paths::RustyCodePath::skills_dir() {
        if user_dir.exists() {
            builder = builder.user_skills_dir(&user_dir);
        }
    }

    builder.build()
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

/// Print skill information
fn print_skill(skill: &rustycode_skill::types::SkillDefinition, detailed: bool) {
    println!("  {} — {}", skill.name, skill.description);

    if detailed {
        println!("    Source: {:?}", skill.source);
        println!("    Activation: {:?}", skill.activation.mode);
        println!("    Effort: {:?}", skill.effort);

        if !skill.allowed_tools.is_empty() {
            println!("    Allowed Tools: {}", skill.allowed_tools.join(", "));
        }

        if let Some(model) = &skill.model_override {
            println!("    Model Override: {}", model);
        }

        println!();
    }
}
