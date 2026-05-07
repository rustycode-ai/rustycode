use crate::{Tool, ToolContext, ToolOutput, ToolPermission};
use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

pub struct SkillTool;

impl Tool for SkillTool {
    fn name(&self) -> &'static str {
        "skill"
    }

    fn description(&self) -> &'static str {
        r#"Execute a skill within the main conversation

When users reference a "slash command" or "/<something>", they are referring to a skill. Use this tool to invoke it.

How to invoke:
- Use this tool with the skill name and optional arguments
- Examples:
  - `skill: "commit"` - invoke the commit skill
  - `skill: "review", args: "src/main.rs"` - invoke with arguments
  - `skill: "tdd"` - invoke the TDD skill

Important:
- Available skills are listed in system-reminder messages in the conversation
- When a skill matches the user's request, invoke it BEFORE generating any other response
- Do not invoke a skill that is already running
- Do not use this tool for built-in CLI commands (like /help, /clear)"#
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::None
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["skill"],
            "properties": {
                "skill": {
                    "type": "string",
                    "description": "Name of the skill to invoke"
                },
                "args": {
                    "type": "string",
                    "description": "Optional arguments to pass to the skill"
                }
            }
        })
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let skill_name = params
            .get("skill")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing skill name"))?;

        let args = params.get("args").and_then(Value::as_str).unwrap_or("");

        // Search for the skill in standard locations
        let skill_content = find_and_load_skill(skill_name, &ctx.cwd)?;

        let mut output = skill_content;
        if !args.is_empty() {
            output = format!("---\nSkill: {skill_name}\nArgs: {args}\n---\n\n{output}");
        }

        Ok(ToolOutput::with_structured(
            output.clone(),
            json!({
                "skill": skill_name,
                "args": args,
                "loaded": true,
            }),
        ))
    }
}

/// Search directories for skill definitions and load content
fn find_and_load_skill(name: &str, cwd: &std::path::Path) -> Result<String> {
    let search_dirs = skill_search_dirs(cwd);

    for dir in &search_dirs {
        // Direct file match: <dir>/<name>.md
        let direct = dir.join(format!("{name}.md"));
        if direct.exists() {
            return fs::read_to_string(&direct)
                .map_err(|e| anyhow!("Failed to read skill {name}: {e}"));
        }

        // Directory match: <dir>/<name>/SKILL.md or <dir>/<name>/<name>.md
        let dir_match = dir.join(name);
        if dir_match.is_dir() {
            for candidate in ["SKILL.md", "skill.md", &format!("{name}.md")] {
                let path = dir_match.join(candidate);
                if path.exists() {
                    return fs::read_to_string(&path)
                        .map_err(|e| anyhow!("Failed to read skill {name}: {e}"));
                }
            }
        }
    }

    Err(anyhow!(
        "Skill '{name}' not found. Searched: {}",
        search_dirs
            .iter()
            .filter(|d| d.exists())
            .map(|d| d.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

/// Build list of directories to search for skills
fn skill_search_dirs(cwd: &std::path::Path) -> Vec<PathBuf> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));

    vec![
        cwd.join(".omc").join("skills"),
        cwd.join(".claude").join("skills"),
        cwd.join("skills"),
        home.join(".omc").join("skills"),
        home.join(".claude").join("skills"),
    ]
}

/// List all available skills from search directories (for discovery prompts)
pub fn list_available_skills(cwd: &std::path::Path) -> Vec<(String, String)> {
    let search_dirs = skill_search_dirs(cwd);
    let mut skills = Vec::new();

    for dir in &search_dirs {
        if !dir.exists() {
            continue;
        }
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = if path.is_dir() {
                    path.file_name().map(|n| n.to_string_lossy().into_owned())
                } else if path.extension().is_some_and(|e| e == "md") {
                    path.file_stem().map(|n| n.to_string_lossy().into_owned())
                } else {
                    None
                };
                if let Some(name) = name {
                    // Extract first line as description
                    let desc = extract_skill_description(&path).unwrap_or_default();
                    skills.push((name, desc));
                }
            }
        }
    }

    skills.sort_by(|a, b| a.0.cmp(&b.0));
    skills.dedup_by(|a, b| a.0 == b.0);
    skills
}

fn extract_skill_description(path: &std::path::Path) -> Option<String> {
    if path.is_dir() {
        for candidate in ["SKILL.md", "skill.md"] {
            let f = path.join(candidate);
            if f.exists() {
                return first_non_heading_line(&f);
            }
        }
        None
    } else {
        first_non_heading_line(path)
    }
}

fn first_non_heading_line(path: &std::path::Path) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    content
        .lines()
        .find(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with("---"))
        .map(|l| {
            if l.len() > 200 {
                let end = l.floor_char_boundary(200);
                format!("{}...", &l[..end])
            } else {
                l.to_string()
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn test_ctx() -> ToolContext {
        ToolContext::new("/tmp")
    }

    #[test]
    fn test_skill_tool_metadata() {
        let tool = SkillTool;
        assert_eq!(tool.name(), "skill");
        assert_eq!(tool.permission(), ToolPermission::None);
        assert!(tool.description().contains("slash command"));
    }

    #[test]
    fn test_skill_tool_missing_name() {
        let tool = SkillTool;
        let result = tool.execute(json!({}), &test_ctx());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("skill name"));
    }

    #[test]
    fn test_skill_tool_not_found() {
        let tool = SkillTool;
        let result = tool.execute(json!({"skill": "nonexistent_skill_xyz"}), &test_ctx());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_skill_tool_loads_md_file() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join(".omc").join("skills");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("test-skill.md"),
            "# Test Skill\nDo the thing.",
        )
        .unwrap();

        let tool = SkillTool;
        let ctx = ToolContext::new(dir.path().to_str().unwrap());
        let result = tool.execute(json!({"skill": "test-skill"}), &ctx);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.text.contains("Do the thing."));
    }

    #[test]
    fn test_skill_tool_with_args() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join(".claude").join("skills");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("my-skill.md"), "# My Skill\nContent here.").unwrap();

        let tool = SkillTool;
        let ctx = ToolContext::new(dir.path().to_str().unwrap());
        let result = tool.execute(json!({"skill": "my-skill", "args": "src/main.rs"}), &ctx);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.text.contains("src/main.rs"));
    }

    #[test]
    fn test_list_available_skills() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join(".omc").join("skills");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("alpha.md"), "# Alpha\nFirst skill.").unwrap();
        fs::write(skill_dir.join("beta.md"), "# Beta\nSecond skill.").unwrap();

        let skills = list_available_skills(dir.path());
        let names: Vec<&str> = skills.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"alpha"), "missing alpha in {names:?}");
        assert!(names.contains(&"beta"), "missing beta in {names:?}");
    }

    #[test]
    fn test_skill_loads_from_subdirectory() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join(".omc").join("skills").join("my-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "# My Skill\nNested content.").unwrap();

        let tool = SkillTool;
        let ctx = ToolContext::new(dir.path().to_str().unwrap());
        let result = tool.execute(json!({"skill": "my-skill"}), &ctx);
        assert!(result.is_ok());
        assert!(result.unwrap().text.contains("Nested content."));
    }
}
