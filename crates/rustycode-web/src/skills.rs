//! Skill management module for web version
//!
//! Provides skill state management compatible with WASM constraints.

use serde::{Deserialize, Serialize};

/// Skill category
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SkillCategory {
    Editor,
    Git,
    Testing,
    Deployment,
    Analysis,
    Custom,
}

/// Skill status for web display
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WebSkillStatus {
    Inactive,
    Active,
    Running,
    Error(String),
}

/// A skill available in the web version
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSkill {
    pub name: String,
    pub description: String,
    pub category: SkillCategory,
    pub status: WebSkillStatus,
    pub auto_enabled: bool,
    pub triggers: Vec<String>,
    pub run_count: usize,
}

/// Skill manager for web - maintains skill state in memory
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WebSkillManager {
    pub skills: Vec<WebSkill>,
    pub selected_index: usize,
}

impl WebSkillManager {
    pub fn new() -> Self {
        Self {
            skills: Self::default_skills(),
            selected_index: 0,
        }
    }

    fn default_skills() -> Vec<WebSkill> {
        vec![
            WebSkill {
                name: "code-review".to_string(),
                description: "Review code changes and suggest improvements".to_string(),
                category: SkillCategory::Analysis,
                status: WebSkillStatus::Inactive,
                auto_enabled: false,
                triggers: vec!["manual".to_string()],
                run_count: 0,
            },
            WebSkill {
                name: "write-tests".to_string(),
                description: "Generate unit tests for selected code".to_string(),
                category: SkillCategory::Testing,
                status: WebSkillStatus::Inactive,
                auto_enabled: false,
                triggers: vec!["manual".to_string()],
                run_count: 0,
            },
            WebSkill {
                name: "explain-code".to_string(),
                description: "Explain what selected code does".to_string(),
                category: SkillCategory::Analysis,
                status: WebSkillStatus::Inactive,
                auto_enabled: false,
                triggers: vec!["manual".to_string()],
                run_count: 0,
            },
            WebSkill {
                name: "git-commit".to_string(),
                description: "Stage and commit changes with smart message".to_string(),
                category: SkillCategory::Git,
                status: WebSkillStatus::Inactive,
                auto_enabled: false,
                triggers: vec!["manual".to_string()],
                run_count: 0,
            },
            WebSkill {
                name: "refactor".to_string(),
                description: "Suggest and apply code refactorings".to_string(),
                category: SkillCategory::Editor,
                status: WebSkillStatus::Inactive,
                auto_enabled: false,
                triggers: vec!["manual".to_string()],
                run_count: 0,
            },
            WebSkill {
                name: "deploy".to_string(),
                description: "Deploy application to target environment".to_string(),
                category: SkillCategory::Deployment,
                status: WebSkillStatus::Inactive,
                auto_enabled: false,
                triggers: vec!["manual".to_string()],
                run_count: 0,
            },
        ]
    }

    pub fn list_skills(&self) -> String {
        if self.skills.is_empty() {
            return "No skills available.".to_string();
        }

        let mut output = String::from("╶─ Skills ─╴\n\n");

        // Group by category
        let mut categories: std::collections::HashMap<String, Vec<&WebSkill>> =
            std::collections::HashMap::new();
        for skill in &self.skills {
            let cat = format!("{:?}", skill.category);
            categories.entry(cat).or_default().push(skill);
        }

        let mut sorted_cats: Vec<_> = categories.iter().collect();
        sorted_cats.sort_by(|a, b| a.0.cmp(b.0));

        for (cat, skills) in sorted_cats {
            output.push_str(&format!("[{}]\n", cat));
            for skill in skills {
                let status_icon = match &skill.status {
                    WebSkillStatus::Inactive => "○",
                    WebSkillStatus::Active => "⚡",
                    WebSkillStatus::Running => "◐",
                    WebSkillStatus::Error(_) => "✗",
                };
                let auto = if skill.auto_enabled { " [auto]" } else { "" };
                output.push_str(&format!("  {} {}{}\n", status_icon, skill.name, auto));
                output.push_str(&format!("      {}\n", skill.description));
            }
            output.push('\n');
        }

        output
    }

    pub fn activate_skill(&mut self, name: &str) -> Result<String, String> {
        let skill = self.skills.iter_mut().find(|s| s.name == name);
        match skill {
            Some(s) => {
                s.auto_enabled = true;
                s.status = WebSkillStatus::Active;
                Ok(format!("Skill '{}' activated", name))
            }
            None => Err(format!("Skill '{}' not found", name)),
        }
    }

    pub fn deactivate_skill(&mut self, name: &str) -> Result<String, String> {
        let skill = self.skills.iter_mut().find(|s| s.name == name);
        match skill {
            Some(s) => {
                s.auto_enabled = false;
                s.status = WebSkillStatus::Inactive;
                Ok(format!("Skill '{}' deactivated", name))
            }
            None => Err(format!("Skill '{}' not found", name)),
        }
    }

    pub fn run_skill(&mut self, name: &str) -> Result<String, String> {
        let skill = self.skills.iter_mut().find(|s| s.name == name);
        match skill {
            Some(s) => {
                s.status = WebSkillStatus::Running;
                s.run_count += 1;
                Ok(format!(
                    "Skill '{}' execution requires tool-server.\n\
                    The skill will be called via HTTP when tool-server is available.",
                    name
                ))
            }
            None => Err(format!("Skill '{}' not found", name)),
        }
    }

    pub fn get_skills_for_panel(&self) -> String {
        let mut lines = vec!["╶─ Skill Browser ─╴".to_string(), "".to_string()];

        for (i, skill) in self.skills.iter().enumerate() {
            let marker = if i == self.selected_index { "▸" } else { " " };
            let status = match &skill.status {
                WebSkillStatus::Inactive => "○",
                WebSkillStatus::Active => "⚡",
                WebSkillStatus::Running => "◐",
                WebSkillStatus::Error(_) => "✗",
            };
            let auto = if skill.auto_enabled { " [A]" } else { "" };
            lines.push(format!("{} {} {}{}", marker, status, skill.name, auto));
            lines.push(format!("   {}", skill.description));
            lines.push(String::new());
        }

        lines.push("─".repeat(30));
        lines.push("↑↓ Navigate  Enter Activate  /run <name> Execute  Esc Back".to_string());

        lines.join("\n")
    }

    #[allow(dead_code)]
    pub fn select_next(&mut self) {
        if !self.skills.is_empty() {
            self.selected_index = (self.selected_index + 1) % self.skills.len();
        }
    }

    #[allow(dead_code)]
    pub fn select_previous(&mut self) {
        if !self.skills.is_empty() {
            self.selected_index = (self.selected_index + self.skills.len() - 1) % self.skills.len();
        }
    }

    #[allow(dead_code)]
    pub fn get_selected_skill(&self) -> Option<&WebSkill> {
        self.skills.get(self.selected_index)
    }
}
