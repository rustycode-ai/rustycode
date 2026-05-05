//! Skill management slash commands

use crate::skills::{SkillStateManager, TriggerCondition};
use std::sync::{Arc, RwLock};

/// Handle skill commands
pub fn handle_skill_command(
    input: &str,
    skill_manager: &Arc<RwLock<SkillStateManager>>,
) -> Result<Option<String>, String> {
    let parts: Vec<&str> = input.split_whitespace().collect();

    if parts.len() < 2 {
        return Ok(Some(
            "Usage: /skill <list|install|uninstall|activate|deactivate|update|run|info|reload>\n\
             Or: /skills (opens skill browser)"
                .to_string(),
        ));
    }

    let subcommand = parts[1];

    match subcommand {
        "list" => cmd_list_skills(skill_manager),
        "install" => cmd_install_skill(&parts[1..]),
        "uninstall" => cmd_uninstall_skill(&parts[1..]),
        "activate" => cmd_activate_skill(&parts[1..], skill_manager),
        "deactivate" | "disable" => cmd_deactivate_skill(&parts[1..], skill_manager),
        "update" => cmd_update_skill(&parts[1..]),
        "run" => cmd_run_skill(&parts[1..], skill_manager),
        "info" => cmd_skill_info(&parts[1..], skill_manager),
        "reload" => cmd_reload_skills(skill_manager),
        _ => Ok(Some(format!(
            "Unknown skill command: {}\n\
             Usage: /skill <list|install|uninstall|activate|deactivate|update|run|info|reload>",
            subcommand
        ))),
    }
}

/// List all available skills with lifecycle states
fn cmd_list_skills(
    skill_manager: &Arc<RwLock<SkillStateManager>>,
) -> Result<Option<String>, String> {
    let skill_manager = skill_manager
        .read()
        .map_err(|e| format!("Lock error: {}", e))?;

    if skill_manager.skills.is_empty() {
        let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
        let skills_path = home.join(".claude").join("skills");
        return Ok(Some(format!(
            "No skills found in {:?}.\n\nUse /skill reload to load skills from disk.",
            skills_path
        )));
    }

    let mut output = format!("Skills ({})\n", skill_manager.skills.len());
    output.push_str("─".repeat(80).as_str());
    output.push('\n');

    // Group by category
    let mut categories: std::collections::HashMap<
        String,
        Vec<&crate::skills::manager::SkillState>,
    > = std::collections::HashMap::new();

    for skill in &skill_manager.skills {
        let category = format!("{:?}", skill.base.category);
        categories.entry(category).or_default().push(skill);
    }

    let mut categories_vec: Vec<_> = categories.iter().collect();
    categories_vec.sort_by(|(k1, _), (k2, _)| k1.cmp(k2));

    for (category, skills) in categories_vec {
        output.push_str(&format!("\n{}\n", category));
        for skill in skills.iter() {
            // Create status indicator
            let (icon, status_text) = if skill.auto_enabled {
                ("⚡", "Active ✓")
            } else {
                ("🧩", "Installed")
            };

            output.push_str(&format!(
                "  {} {:20} [{}]\n",
                icon, skill.base.name, status_text
            ));

            if !skill.base.description.is_empty() {
                output.push_str(&format!("     {}\n", skill.base.description));
            }
        }
    }

    output.push_str(&format!(
        "\n💡 Active: {} / {} skills\n",
        skill_manager.active_count(),
        skill_manager.skill_count()
    ));
    output.push_str("Commands: /skill <install|uninstall|activate|deactivate|update|info> [name]");

    Ok(Some(output))
}

/// Activate a skill for auto-triggering
fn cmd_activate_skill(
    parts: &[&str],
    skill_manager: &Arc<RwLock<SkillStateManager>>,
) -> Result<Option<String>, String> {
    if parts.len() < 2 {
        return Ok(Some("Usage: /skill activate <name>".to_string()));
    }

    let name = parts[1];
    let mut skill_manager = skill_manager
        .write()
        .map_err(|e| format!("Lock error: {}", e))?;

    match skill_manager.activate_skill(name) {
        Ok(()) => Ok(Some(format!(
            "✓ Skill '{}' is now active and will auto-trigger",
            name
        ))),
        Err(e) => Ok(Some(format!("❌ {}", e))),
    }
}

/// Deactivate a skill
fn cmd_deactivate_skill(
    parts: &[&str],
    skill_manager: &Arc<RwLock<SkillStateManager>>,
) -> Result<Option<String>, String> {
    if parts.len() < 2 {
        return Ok(Some("Usage: /skill deactivate <name>".to_string()));
    }

    let name = parts[1];
    let mut skill_manager = skill_manager
        .write()
        .map_err(|e| format!("Lock error: {}", e))?;

    match skill_manager.deactivate_skill(name) {
        Ok(()) => Ok(Some(format!(
            "✓ Skill '{}' deactivated. It will not auto-trigger.",
            name
        ))),
        Err(e) => Ok(Some(format!("❌ {}", e))),
    }
}

/// Run a skill immediately
fn cmd_run_skill(
    parts: &[&str],
    skill_manager: &Arc<RwLock<SkillStateManager>>,
) -> Result<Option<String>, String> {
    if parts.len() < 2 {
        return Ok(Some("Usage: /skill run <name> [args...]".to_string()));
    }

    let name = parts[1];
    let args: Vec<String> = parts[2..].iter().map(|s| s.to_string()).collect();

    // Find the skill
    let skill_manager_read = skill_manager
        .read()
        .map_err(|e| format!("Lock error: {}", e))?;
    let skill = skill_manager_read.find_skill(name).ok_or_else(|| {
        format!(
            "Skill '{}' not found. Use /skill list to see available skills.",
            name
        )
    })?;

    Ok(Some(format!(
        "⚠️ Skill execution is not yet implemented.\n\n\
         Skill: {}\n\
         Description: {}\n\
         Args: {}\n\
         \n\
         Use /skill info {} for details about this skill.",
        skill.base.name,
        skill.base.description,
        if args.is_empty() {
            "(none)".to_string()
        } else {
            args.join(" ")
        },
        name
    )))
}

/// Show detailed information about a skill
fn cmd_skill_info(
    parts: &[&str],
    skill_manager: &Arc<RwLock<SkillStateManager>>,
) -> Result<Option<String>, String> {
    if parts.len() < 2 {
        return Ok(Some("Usage: /skill info <name>".to_string()));
    }

    let name = parts[1];
    let skill_manager = skill_manager
        .read()
        .map_err(|e| format!("Lock error: {}", e))?;

    let skill = skill_manager.find_skill(name).ok_or_else(|| {
        format!(
            "Skill '{}' not found. Use /skill list to see available skills.",
            name
        )
    })?;

    let mut output = format!("📋 Skill: {}\n\n", skill.base.name);

    if !skill.base.description.is_empty() {
        output.push_str(&format!("Description:\n  {}\n\n", skill.base.description));
    }

    // State and status
    let (icon, state_text) = if skill.auto_enabled {
        ("⚡", "Active (auto-triggering enabled)")
    } else {
        ("🧩", "Installed (auto-triggering disabled)")
    };

    output.push_str(&format!(
        "State: {} {}\n\
         Category: {:?}\n\n",
        icon, state_text, skill.base.category
    ));

    // Triggers
    output.push_str("Triggers:\n");
    for trigger in &skill.triggers {
        let enabled = match trigger {
            TriggerCondition::OnCommit => "✓",
            TriggerCondition::OnFileChange => "✗",
            TriggerCondition::OnError => "✓",
            TriggerCondition::ManualOnly => "✗",
        };
        output.push_str(&format!("  {} {:?}\n", enabled, trigger));
    }

    // Statistics
    output.push_str(&format!(
        "\nLast run: {}\n\
         Run count: {} times\n\
         Success rate: {:.1}%",
        skill.last_run_display(),
        skill.run_count,
        if skill.run_count > 0 {
            (((skill.run_count - skill.error_count) as f64) / (skill.run_count as f64)) * 100.0
        } else {
            100.0
        }
    ));

    // Installation info
    if let Some(installation) = &skill.base.path.to_str() {
        output.push_str(&format!("\nPath: {}", installation));
    }

    // Lifecycle commands
    output.push_str(&format!(
        "\n\nCommands:\n\
         [Enter] Run now  [Space] {}  [u] Update  [un] Uninstall",
        if skill.auto_enabled {
            "Deactivate"
        } else {
            "Activate"
        }
    ));

    Ok(Some(output))
}

/// Reload skills from disk
fn cmd_reload_skills(
    skill_manager: &Arc<RwLock<SkillStateManager>>,
) -> Result<Option<String>, String> {
    let mut skill_manager = skill_manager
        .write()
        .map_err(|e| format!("Lock error: {}", e))?;

    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    let skills_path = home.join(".claude").join("skills");

    // Create a new runtime since TUI render thread has no Tokio context
    tokio::runtime::Runtime::new()
        .expect("Failed to create runtime")
        .block_on(skill_manager.load_skills())
        .map_err(|e| format!("Failed to reload skills: {}", e))?;

    let count = skill_manager.skill_count();
    if count == 0 {
        Ok(Some(format!(
            "No skills found in {:?}.\n\nSkills should be installed as subdirectories with SKILL.md files.",
            skills_path
        )))
    } else {
        Ok(Some(format!(
            "✓ Reloaded {} skills from {:?}",
            count, skills_path
        )))
    }
}

/// Install a skill from marketplace
fn cmd_install_skill(parts: &[&str]) -> Result<Option<String>, String> {
    if parts.len() < 2 {
        return Ok(Some("Usage: /skill install <name>".to_string()));
    }

    let name = parts[1];

    match tokio::runtime::Runtime::new()
        .expect("Failed to create runtime")
        .block_on(crate::skills::install_skill(name))
    {
        Ok(_) => Ok(Some(format!(
            "✓ Successfully installed skill '{}'\n\
             Use /skill activate {} to enable auto-triggering",
            name, name
        ))),
        Err(e) => Ok(Some(format!(
            "❌ Failed to install skill '{}': {}",
            name, e
        ))),
    }
}

/// Uninstall a skill
fn cmd_uninstall_skill(parts: &[&str]) -> Result<Option<String>, String> {
    if parts.len() < 2 {
        return Ok(Some("Usage: /skill uninstall <name>".to_string()));
    }

    let name = parts[1];

    // Check if installed
    if !crate::skills::is_installed(name) {
        return Ok(Some(format!("❌ Skill '{}' is not installed", name)));
    }

    match tokio::runtime::Runtime::new()
        .expect("Failed to create runtime")
        .block_on(crate::skills::uninstall_skill(name))
    {
        Ok(_) => Ok(Some(format!("✓ Successfully uninstalled skill '{}'", name))),
        Err(e) => Ok(Some(format!(
            "❌ Failed to uninstall skill '{}': {}",
            name, e
        ))),
    }
}

/// Update a skill or all skills
fn cmd_update_skill(parts: &[&str]) -> Result<Option<String>, String> {
    let name = if parts.len() >= 2 {
        Some(parts[1])
    } else {
        None
    };

    if let Some(skill_name) = name {
        match tokio::runtime::Runtime::new()
            .expect("Failed to create runtime")
            .block_on(crate::skills::update_skill(skill_name))
        {
            Ok(info) => {
                if info.update_available {
                    Ok(Some(format!(
                        "✓ Updated '{}' from {} to {}",
                        skill_name, info.current_version, info.latest_version
                    )))
                } else {
                    Ok(Some(format!(
                        "✓ Skill '{}' is already up to date",
                        skill_name
                    )))
                }
            }
            Err(e) => Ok(Some(format!(
                "❌ Failed to update skill '{}': {}",
                skill_name, e
            ))),
        }
    } else {
        match tokio::runtime::Runtime::new()
            .expect("Failed to create runtime")
            .block_on(crate::skills::update_all_skills())
        {
            Ok(updates) => {
                if updates.is_empty() {
                    Ok(Some("✓ All skills are already up to date".to_string()))
                } else {
                    Ok(Some(format!(
                        "✓ Updated {} skills:\n{}",
                        updates.len(),
                        updates
                            .iter()
                            .map(|u| format!(
                                "  • {}: {} → {}",
                                u.name, u.current_version, u.latest_version
                            ))
                            .collect::<Vec<_>>()
                            .join("\n")
                    )))
                }
            }
            Err(e) => Ok(Some(format!("❌ Failed to update skills: {}", e))),
        }
    }
}

/// Handle /skills command (opens browser)
pub fn handle_skills_browser(
    skill_manager: &Arc<RwLock<SkillStateManager>>,
) -> Result<Option<String>, String> {
    // For now, just show the list
    // In a full implementation, this would open a TUI browser
    cmd_list_skills(skill_manager)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_skill_command_help() {
        let skill_manager = Arc::new(RwLock::new(SkillStateManager::new()));

        let result = handle_skill_command("/skill", &skill_manager);
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(output.is_some());
        assert!(output.unwrap().contains("Usage:"));
    }
}
