//! Skillify slash command — create a new skill from conversation context
//!
//! Usage: `/skillify <name> [description]`
//!
//! Creates a SKILL.md file with proper frontmatter in either the project-level
//! `.rustycode/skills/` directory or the user-level `~/.rustycode/skills/` directory.

use rustycode_skill::bundled::{write_skill_to_dir, SkillifyBuilder};
use std::path::Path;

/// Handle `/skillify` command.
///
/// Accepts `/skillify <name> [description...]` syntax.
/// Returns a success message with the file path, or an error message.
pub fn handle_skillify_command(input: &str, cwd: &Path) -> Result<Option<String>, String> {
    // Parse: skip the command itself (first token), then take name and optional description
    let parts: Vec<&str> = input.split_whitespace().collect();

    // parts[0] might be "/skillify" or empty if input was pre-stripped
    let args_start = if parts.first().map(|s| *s == "/skillify").unwrap_or(false) {
        1
    } else {
        0
    };

    let args = &parts[args_start..];

    if args.is_empty() {
        return Ok(Some("Usage: /skillify <name> [description]".to_string()));
    }

    let name = args[0];

    // Validate skill name: alphanumeric, hyphens, underscores only
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(
            "Skill name must contain only alphanumeric characters, hyphens, or underscores"
                .to_string(),
        );
    }

    let description = if args.len() > 1 {
        args[1..].join(" ")
    } else {
        String::new()
    };

    // Determine target directory: project-level first, then user-level
    let base_dir = determine_skill_base_dir(cwd)?;

    // Build and write the skill
    let mut builder = SkillifyBuilder::new(name);
    if !description.is_empty() {
        builder = builder.description(&description);
    }

    let skill_def = builder.build();

    let result = write_skill_to_dir(&skill_def, &base_dir)
        .map_err(|e| format!("Failed to write skill: {}", e))?;

    Ok(Some(format!(
        "✓ Skill '{}' created at {}\nEdit the SKILL.md file to add steps, tools, and instructions.",
        result.skill_id,
        result.path.display()
    )))
}

/// Determine where skills should be written.
///
/// Prefers `.rustycode/skills/` in the current working directory (project-level).
/// Falls back to `~/.rustycode/skills/` (user-level).
fn determine_skill_base_dir(cwd: &Path) -> Result<std::path::PathBuf, String> {
    let project_skills = cwd.join(".rustycode").join("skills");

    // If .rustycode exists or can be created, use project-level
    if cwd.join(".rustycode").exists() {
        return Ok(project_skills);
    }

    // Fall back to user-level
    let home = dirs::home_dir().ok_or_else(|| "Cannot determine home directory".to_string())?;
    Ok(home.join(".rustycode").join("skills"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skillify_no_args_shows_usage() {
        let cwd = std::env::current_dir().unwrap();
        let result = handle_skillify_command("/skillify", &cwd).unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().contains("Usage:"));
    }

    #[test]
    fn test_skillify_invalid_name() {
        let cwd = std::env::current_dir().unwrap();
        let result = handle_skillify_command("/skillify bad!name", &cwd);
        assert!(result.is_err());
    }

    #[test]
    fn test_determine_base_dir_prefers_project() {
        let tmp = tempfile::tempdir().unwrap();
        let dot_dir = tmp.path().join(".rustycode");
        std::fs::create_dir_all(&dot_dir).unwrap();
        let base = determine_skill_base_dir(tmp.path()).unwrap();
        assert_eq!(base, tmp.path().join(".rustycode").join("skills"));
    }

    #[test]
    fn test_determine_base_dir_falls_back_to_home() {
        let tmp = tempfile::tempdir().unwrap();
        // No .rustycode dir, so should fall back to home
        let base = determine_skill_base_dir(tmp.path()).unwrap();
        let home = dirs::home_dir().unwrap();
        assert_eq!(base, home.join(".rustycode").join("skills"));
    }
}
