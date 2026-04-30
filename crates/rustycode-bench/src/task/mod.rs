//! Task configuration parser for Harbor task.toml format.

pub mod steps;

use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};

/// Parsed task configuration from Harbor task.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskConfig {
    /// Task metadata (author, difficulty, category).
    #[serde(default)]
    pub metadata: TaskMetadata,
    /// Verifier configuration.
    #[serde(default)]
    pub verifier: VerifierConfig,
    /// Agent configuration.
    #[serde(default)]
    pub agent: AgentConfig,
    /// Environment configuration.
    #[serde(default)]
    pub environment: EnvironmentConfig,
}

/// Task metadata section.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskMetadata {
    #[serde(default)]
    pub author_name: String,
    #[serde(default)]
    pub author_email: String,
    #[serde(default)]
    pub difficulty: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Verifier configuration section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifierConfig {
    #[serde(default = "default_timeout")]
    pub timeout_sec: f64,
}

impl Default for VerifierConfig {
    fn default() -> Self {
        Self {
            timeout_sec: default_timeout(),
        }
    }
}

/// Agent configuration section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    #[serde(default = "default_timeout")]
    pub timeout_sec: f64,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            timeout_sec: default_timeout(),
        }
    }
}

/// Environment configuration section from task.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentConfig {
    #[serde(default = "default_build_timeout")]
    pub build_timeout_sec: f64,
    #[serde(default)]
    pub docker_image: Option<String>,
    #[serde(default = "default_cpus")]
    pub cpus: u32,
    /// Memory limit as a string (e.g. "2G", "512M").
    #[serde(default = "default_memory")]
    pub memory: String,
    #[serde(default)]
    pub storage: Option<String>,
}

impl Default for EnvironmentConfig {
    fn default() -> Self {
        Self {
            build_timeout_sec: default_build_timeout(),
            docker_image: None,
            cpus: default_cpus(),
            memory: default_memory(),
            storage: None,
        }
    }
}

const fn default_timeout() -> f64 {
    900.0
}

const fn default_build_timeout() -> f64 {
    600.0
}

const fn default_cpus() -> u32 {
    1
}

fn default_memory() -> String {
    "2G".to_string()
}

/// Resolved task with all paths and configuration.
#[derive(Debug, Clone)]
pub struct ResolvedTask {
    /// Unique task name (e.g. "sparql-university").
    pub name: String,
    /// Root directory of the task.
    pub task_dir: PathBuf,
    /// Parsed task.toml configuration.
    pub config: TaskConfig,
    /// Task instruction text (from instruction.md).
    pub instruction: String,
    /// Path to the environment directory (contains Dockerfile).
    pub environment_dir: PathBuf,
    /// Path to the tests directory.
    pub tests_dir: PathBuf,
    /// Path to the solution directory (oracle).
    pub solution_dir: PathBuf,
}

impl ResolvedTask {
    /// Resolve a task from its root directory.
    ///
    /// Expected structure:
    /// ```text
    /// task-name/
    /// ├── task.toml
    /// ├── instruction.md
    /// ├── environment/
    /// │   └── Dockerfile
    /// ├── tests/
    /// │   └── test.sh
    /// └── solution/
    ///     └── solve.sh
    /// ```
    pub fn from_dir(task_dir: &Path) -> anyhow::Result<Self> {
        let name = task_dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        // Parse task.toml
        let toml_path = task_dir.join("task.toml");
        let toml_content = std::fs::read_to_string(&toml_path)
            .with_context(|| format!("reading task.toml from {}", toml_path.display()))?;
        let config: TaskConfig = toml::from_str(&toml_content)
            .with_context(|| format!("parsing task.toml from {}", toml_path.display()))?;

        // Read instruction.md
        let instruction_path = task_dir.join("instruction.md");
        let instruction = std::fs::read_to_string(&instruction_path).with_context(|| {
            format!("reading instruction.md from {}", instruction_path.display())
        })?;

        let environment_dir = task_dir.join("environment");
        let tests_dir = task_dir.join("tests");
        let solution_dir = task_dir.join("solution");

        // Validate required paths (Dockerfile is optional for native mode)
        if !environment_dir.join("Dockerfile").exists() {
            tracing::debug!(
                "No Dockerfile at {} (not required for native mode)",
                environment_dir.join("Dockerfile").display()
            );
        }

        Ok(Self {
            name,
            task_dir: task_dir.to_path_buf(),
            config,
            instruction,
            environment_dir,
            tests_dir,
            solution_dir,
        })
    }

    /// Discover all tasks in a dataset directory.
    ///
    /// Handles three layouts:
    /// 1. Single task: dir contains `task.toml` directly
    /// 2. Flat: `dir/{task-name}/task.toml`
    /// 3. Nested (Harbor cache): `dir/{hash}/{task-name}/task.toml`
    pub fn discover(dataset_dir: &Path) -> anyhow::Result<Vec<Self>> {
        // Check if the directory itself is a task
        if dataset_dir.join("task.toml").exists() {
            let task = Self::from_dir(dataset_dir)?;
            return Ok(vec![task]);
        }

        let mut tasks = Vec::new();

        let entries = std::fs::read_dir(dataset_dir)
            .with_context(|| format!("reading dataset dir {}", dataset_dir.display()))?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            // Direct child has task.toml (flat layout)
            if path.join("task.toml").exists() {
                match Self::from_dir(&path) {
                    Ok(task) => tasks.push(task),
                    Err(e) => {
                        tracing::warn!("Skipping task at {}: {}", path.display(), e);
                    }
                }
            } else {
                // Nested layout: scan one level deeper
                let Ok(sub_entries) = std::fs::read_dir(&path) else {
                    continue;
                };
                for sub_entry in sub_entries.flatten() {
                    let sub_path = sub_entry.path();
                    if sub_path.is_dir() && sub_path.join("task.toml").exists() {
                        match Self::from_dir(&sub_path) {
                            Ok(task) => tasks.push(task),
                            Err(e) => {
                                tracing::warn!("Skipping task at {}: {}", sub_path.display(), e);
                            }
                        }
                    }
                }
            }
        }

        tasks.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(tasks)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn task_config_defaults() {
        let toml = "";
        let config: TaskConfig = toml::from_str(toml).unwrap();
        assert!(config.metadata.author_name.is_empty());
        assert!(config.metadata.tags.is_empty());
        assert_eq!(config.verifier.timeout_sec, 900.0);
        assert_eq!(config.agent.timeout_sec, 900.0);
        assert_eq!(config.environment.build_timeout_sec, 600.0);
        assert_eq!(config.environment.cpus, 1);
        assert_eq!(config.environment.memory, "2G");
        assert!(config.environment.docker_image.is_none());
    }

    #[test]
    fn task_config_full_parse() {
        let toml = r#"
[metadata]
author_name = "Alice"
author_email = "alice@example.com"
difficulty = "hard"
category = "systems"
tags = ["rust", "async"]

[verifier]
timeout_sec = 120

[agent]
timeout_sec = 300

[environment]
build_timeout_sec = 60
docker_image = "python:3.12"
cpus = 4
memory = "8G"
storage = "10G"
"#;
        let config: TaskConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.metadata.author_name, "Alice");
        assert_eq!(config.metadata.difficulty, "hard");
        assert_eq!(config.metadata.tags, vec!["rust", "async"]);
        assert_eq!(config.verifier.timeout_sec, 120.0);
        assert_eq!(config.agent.timeout_sec, 300.0);
        assert_eq!(config.environment.build_timeout_sec, 60.0);
        assert_eq!(
            config.environment.docker_image.as_deref(),
            Some("python:3.12")
        );
        assert_eq!(config.environment.cpus, 4);
        assert_eq!(config.environment.memory, "8G");
        assert_eq!(config.environment.storage.as_deref(), Some("10G"));
    }

    #[test]
    fn task_config_serde_roundtrip() {
        let toml = r#"
[metadata]
author_name = "Bob"
difficulty = "easy"
"#;
        let config: TaskConfig = toml::from_str(toml).unwrap();
        let json = serde_json::to_string(&config).unwrap();
        let back: TaskConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.metadata.author_name, "Bob");
        assert_eq!(back.metadata.difficulty, "easy");
    }

    #[test]
    fn resolved_task_from_dir_valid() {
        let dir = tempfile::tempdir().unwrap();
        let task_dir = dir.path().join("my-task");
        std::fs::create_dir_all(task_dir.join("environment")).unwrap();
        std::fs::create_dir_all(task_dir.join("tests")).unwrap();
        std::fs::create_dir_all(task_dir.join("solution")).unwrap();

        // Write task.toml
        std::fs::write(task_dir.join("task.toml"), "").unwrap();
        // Write instruction.md
        std::fs::write(task_dir.join("instruction.md"), "Solve the task").unwrap();
        // Write Dockerfile
        std::fs::write(
            task_dir.join("environment/Dockerfile"),
            "FROM ubuntu:22.04\n",
        )
        .unwrap();

        let task = ResolvedTask::from_dir(&task_dir).unwrap();
        assert_eq!(task.name, "my-task");
        assert_eq!(task.instruction, "Solve the task");
        assert!(task.environment_dir.join("Dockerfile").exists());
        assert!(task.tests_dir.exists());
        assert!(task.solution_dir.exists());
    }

    #[test]
    fn resolved_task_from_dir_missing_toml() {
        let dir = tempfile::tempdir().unwrap();
        let task_dir = dir.path().join("no-toml");
        std::fs::create_dir_all(&task_dir).unwrap();

        let result = ResolvedTask::from_dir(&task_dir);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("task.toml"));
    }

    #[test]
    fn resolved_task_from_dir_missing_instruction() {
        let dir = tempfile::tempdir().unwrap();
        let task_dir = dir.path().join("no-inst");
        std::fs::create_dir_all(task_dir.join("environment")).unwrap();

        std::fs::write(task_dir.join("task.toml"), "").unwrap();
        std::fs::write(task_dir.join("environment/Dockerfile"), "FROM alpine\n").unwrap();
        // No instruction.md

        let result = ResolvedTask::from_dir(&task_dir);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("instruction.md"));
    }

    #[test]
    fn resolved_task_from_dir_works_without_dockerfile() {
        let dir = tempfile::tempdir().unwrap();
        let task_dir = dir.path().join("no-docker");
        std::fs::create_dir_all(task_dir.join("environment")).unwrap();

        std::fs::write(task_dir.join("task.toml"), "").unwrap();
        std::fs::write(task_dir.join("instruction.md"), "Do it").unwrap();
        // No Dockerfile — should still succeed (optional for native mode)

        let result = ResolvedTask::from_dir(&task_dir);
        assert!(result.is_ok());
    }

    #[test]
    fn resolved_task_discover_multiple() {
        let dir = tempfile::tempdir().unwrap();

        // Create three valid tasks (Dockerfile is now optional)
        for name in &["alpha-task", "beta-task", "gamma-task"] {
            let task_dir = dir.path().join(name);
            std::fs::create_dir_all(task_dir.join("environment")).unwrap();
            std::fs::write(task_dir.join("task.toml"), "").unwrap();
            std::fs::write(task_dir.join("instruction.md"), format!("Task {name}")).unwrap();
        }

        let tasks = ResolvedTask::discover(dir.path()).unwrap();
        assert_eq!(tasks.len(), 3);
        // Should be sorted alphabetically
        assert_eq!(tasks[0].name, "alpha-task");
        assert_eq!(tasks[1].name, "beta-task");
        assert_eq!(tasks[2].name, "gamma-task");
    }

    #[test]
    fn resolved_task_discover_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let tasks = ResolvedTask::discover(dir.path()).unwrap();
        assert!(tasks.is_empty());
    }

    #[test]
    fn resolved_task_discover_skips_files() {
        let dir = tempfile::tempdir().unwrap();
        // A plain file (not a directory) should be skipped
        std::fs::write(dir.path().join("not-a-task.txt"), "hello").unwrap();
        let tasks = ResolvedTask::discover(dir.path()).unwrap();
        assert!(tasks.is_empty());
    }

    #[test]
    fn environment_config_default_values() {
        let ec = EnvironmentConfig::default();
        assert_eq!(ec.build_timeout_sec, 600.0);
        assert_eq!(ec.cpus, 1);
        assert_eq!(ec.memory, "2G");
        assert!(ec.docker_image.is_none());
        assert!(ec.storage.is_none());
    }

    #[test]
    fn task_metadata_default_empty() {
        let m = TaskMetadata::default();
        assert!(m.author_name.is_empty());
        assert!(m.author_email.is_empty());
        assert!(m.difficulty.is_empty());
        assert!(m.category.is_empty());
        assert!(m.tags.is_empty());
    }
}
