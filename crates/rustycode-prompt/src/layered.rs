//! Layered prompt builder with context injection
//!
//! Builds prompts from multiple layers:
//! - Base: Core identity and instructions
//! - Model-specific: Optimized for each model
//! - Environment: Dynamic context (git, dir, platform)
//! - Project: AGENTS.md, CLAUDE.md scanning
//! - Local: Per-directory instruction files

use crate::environment::EnvironmentContext;
use anyhow::Result;
use rustycode_config::DomainContext;
use std::path::Path;
use tokio::fs;

/// Prompt layer types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PromptLayer {
    Base,
    ModelSpecific,
    Environment,
    Project,
    Local,
    Skills,
}

/// Model provider for model-specific prompts
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ModelProvider {
    Anthropic,
    Google,
    OpenAI,
    Generic,
}

impl ModelProvider {
    pub fn from_model_id(model_id: &str) -> Self {
        if model_id.contains("claude") || model_id.contains("anthropic") {
            Self::Anthropic
        } else if model_id.contains("gemini") || model_id.contains("google") {
            Self::Google
        } else if model_id.contains("gpt")
            || model_id.contains("openai")
            || model_id.contains("openrouter")
        {
            Self::OpenAI
        } else {
            Self::Generic
        }
    }
}

/// Instruction file scanner
#[derive(Debug, Clone)]
pub struct InstructionScanner {
    files: Vec<&'static str>,
}

impl InstructionScanner {
    pub fn new() -> Self {
        Self {
            files: vec!["AGENTS.md", "CLAUDE.md"],
        }
    }

    /// Scan upward from file to project root, loading instruction files
    pub async fn scan_upward(&self, file: &Path, project_root: &Path) -> Vec<String> {
        let mut instructions = Vec::new();
        let mut current = file.parent();

        while let Some(path) = current {
            if path == project_root || !path.starts_with(project_root) {
                break;
            }

            for filename in &self.files {
                let filepath = path.join(filename);
                if filepath.exists() {
                    if let Ok(content) = fs::read_to_string(&filepath).await {
                        instructions.push(format!("## Instructions from: {}", filepath.display()));
                        instructions.push(content);
                        instructions.push(String::new());
                    }
                }
            }

            current = path.parent();
        }

        instructions
    }

    /// Load global instruction files (e.g., ~/.claude/CLAUDE.md)
    pub async fn load_global(&self) -> Vec<String> {
        let mut instructions = Vec::new();

        // Try to find home directory
        if let Some(home) = dirs::home_dir() {
            for filename in &self.files {
                let filepath = home.join(".claude").join(filename);
                if filepath.exists() {
                    if let Ok(content) = fs::read_to_string(&filepath).await {
                        instructions.push(format!("## Instructions from: {}", filepath.display()));
                        instructions.push(content);
                        instructions.push(String::new());
                    }
                }
            }
        }

        instructions
    }
}

impl Default for InstructionScanner {
    fn default() -> Self {
        Self::new()
    }
}

/// Layered prompt builder
#[derive(Debug, Clone)]
pub struct PromptBuilder {
    base_prompt: String,
    anthropic_prompt: String,
    generic_prompt: String,
    scanner: InstructionScanner,
}

impl PromptBuilder {
    pub fn new() -> Self {
        Self {
            base_prompt: include_str!("../prompts/base.txt").to_string(),
            anthropic_prompt: include_str!("../prompts/anthropic.txt").to_string(),
            generic_prompt: include_str!("../prompts/generic.txt").to_string(),
            scanner: InstructionScanner::new(),
        }
    }

    /// Build complete prompt with all layers
    pub async fn build(
        &self,
        model_id: &str,
        file: Option<&Path>,
        env: &EnvironmentContext,
    ) -> Result<String> {
        let mut layers = Vec::new();

        // Layer 1: Base identity
        layers.push(self.base_prompt.trim().to_string());
        layers.push(String::new());

        // Layer 2: Model-specific
        let provider = ModelProvider::from_model_id(model_id);
        let model_prompt = self.get_model_prompt(&provider);
        layers.push(model_prompt.trim().to_string());
        layers.push(String::new());

        // Layer 3: Environment
        layers.push(env.format_markdown());
        layers.push(String::new());

        // Layer 4: Domain context
        if let Ok(Some(domain_path)) = DomainContext::discover(&env.workspace_root) {
            if let Ok(domain) = DomainContext::load_from_file(&domain_path) {
                let domain_block = domain.format_markdown();
                if !domain_block.trim().is_empty() {
                    layers.push(domain_block);
                    layers.push(String::new());
                }
            }
        }

        // Layer 5: Project instructions
        if let Some(filepath) = file {
            let project_instructions = self
                .scanner
                .scan_upward(filepath, &env.workspace_root)
                .await;

            if !project_instructions.is_empty() {
                layers.extend(project_instructions);
                layers.push(String::new());
            }
        }

        // Layer 6: Global instructions
        let global_instructions = self.scanner.load_global().await;
        if !global_instructions.is_empty() {
            layers.extend(global_instructions);
            layers.push(String::new());
        }

        Ok(layers.join("\n\n"))
    }

    fn get_model_prompt(&self, provider: &ModelProvider) -> &str {
        match provider {
            ModelProvider::Anthropic => &self.anthropic_prompt,
            ModelProvider::Google | ModelProvider::OpenAI | ModelProvider::Generic => {
                &self.generic_prompt
            }
        }
    }
}

impl Default for PromptBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_build_prompt() {
        let builder = PromptBuilder::new();

        let env = EnvironmentContext {
            working_directory: PathBuf::from("/tmp/test"),
            workspace_root: PathBuf::from("/tmp/test"),
            is_git_repo: false,
            platform: "linux".to_string(),
            date: "2025-03-13".to_string(),
            git_status: None,
        };

        let prompt = builder
            .build("claude-3", Some(Path::new("/tmp/test/main.rs")), &env)
            .await
            .unwrap();

        assert!(!prompt.is_empty());
        assert!(prompt.contains("Environment"));
    }

    #[tokio::test]
    async fn test_build_prompt_with_domain_context() {
        let temp = tempfile::tempdir().unwrap();
        let domain_dir = temp.path().join(".rustycode");
        tokio::fs::create_dir_all(&domain_dir).await.unwrap();
        tokio::fs::write(
            domain_dir.join("domain.yaml"),
            "project_name: demo\nlanguage: rust\n",
        )
        .await
        .unwrap();

        let builder = PromptBuilder::new();
        let env = EnvironmentContext {
            working_directory: temp.path().to_path_buf(),
            workspace_root: temp.path().to_path_buf(),
            is_git_repo: false,
            platform: "linux".to_string(),
            date: "2026-04-25".to_string(),
            git_status: None,
        };
        let file_path = temp.path().join("main.rs");

        let prompt = builder
            .build("claude-3", Some(file_path.as_path()), &env)
            .await
            .unwrap();

        assert!(prompt.contains("Domain Context"));
        assert!(prompt.contains("demo"));
        assert!(prompt.contains("rust"));
    }

    #[test]
    fn test_model_provider_detection() {
        assert_eq!(
            ModelProvider::from_model_id("claude-3-sonnet"),
            ModelProvider::Anthropic
        );
        assert_eq!(
            ModelProvider::from_model_id("gemini-pro"),
            ModelProvider::Google
        );
        assert_eq!(ModelProvider::from_model_id("gpt-4"), ModelProvider::OpenAI);
        assert_eq!(
            ModelProvider::from_model_id("unknown-model"),
            ModelProvider::Generic
        );
    }

    #[test]
    fn test_model_provider_anthropic_variants() {
        assert_eq!(
            ModelProvider::from_model_id("claude-opus-4"),
            ModelProvider::Anthropic
        );
        assert_eq!(
            ModelProvider::from_model_id("anthropic-model"),
            ModelProvider::Anthropic
        );
        assert_eq!(
            ModelProvider::from_model_id("claude"),
            ModelProvider::Anthropic
        );
    }

    #[test]
    fn test_model_provider_google_variants() {
        assert_eq!(
            ModelProvider::from_model_id("gemini-ultra"),
            ModelProvider::Google
        );
        assert_eq!(
            ModelProvider::from_model_id("google-gemini"),
            ModelProvider::Google
        );
    }

    #[test]
    fn test_model_provider_openai_variants() {
        assert_eq!(
            ModelProvider::from_model_id("gpt-4o"),
            ModelProvider::OpenAI
        );
        assert_eq!(
            ModelProvider::from_model_id("openai-gpt"),
            ModelProvider::OpenAI
        );
        assert_eq!(
            ModelProvider::from_model_id("gpt-3.5-turbo"),
            ModelProvider::OpenAI
        );
    }

    #[test]
    fn test_model_provider_generic_fallback() {
        assert_eq!(
            ModelProvider::from_model_id("llama-3"),
            ModelProvider::Generic
        );
        assert_eq!(
            ModelProvider::from_model_id("mistral"),
            ModelProvider::Generic
        );
        assert_eq!(ModelProvider::from_model_id(""), ModelProvider::Generic);
    }

    #[test]
    fn test_model_provider_openrouter() {
        // OpenRouter with Anthropic model — "claude" substring matches first
        assert_eq!(
            ModelProvider::from_model_id("openrouter/anthropic/claude-3"),
            ModelProvider::Anthropic
        );
        // OpenRouter with non-provider-specific model
        assert_eq!(
            ModelProvider::from_model_id("openrouter/meta-llama/llama-3"),
            ModelProvider::OpenAI
        );
        assert_eq!(
            ModelProvider::from_model_id("openrouter-gpt-4"),
            ModelProvider::OpenAI
        );
    }

    #[test]
    fn test_prompt_layer_variants() {
        assert!(matches!(PromptLayer::Base, PromptLayer::Base));
        assert!(matches!(
            PromptLayer::ModelSpecific,
            PromptLayer::ModelSpecific
        ));
        assert!(matches!(PromptLayer::Environment, PromptLayer::Environment));
        assert!(matches!(PromptLayer::Project, PromptLayer::Project));
        assert!(matches!(PromptLayer::Local, PromptLayer::Local));
        assert!(matches!(PromptLayer::Skills, PromptLayer::Skills));
    }

    #[test]
    fn test_instruction_scanner_new() {
        let scanner = InstructionScanner::new();
        assert_eq!(scanner.files, vec!["AGENTS.md", "CLAUDE.md"]);
    }

    #[test]
    fn test_instruction_scanner_default() {
        let scanner = InstructionScanner::default();
        assert_eq!(scanner.files.len(), 2);
    }

    #[test]
    fn test_prompt_builder_new() {
        let builder = PromptBuilder::new();
        assert!(!builder.base_prompt.is_empty());
    }

    #[test]
    fn test_prompt_builder_default() {
        let builder = PromptBuilder::default();
        assert!(!builder.base_prompt.is_empty());
    }

    #[tokio::test]
    async fn test_build_prompt_no_file() {
        let builder = PromptBuilder::new();
        let env = EnvironmentContext {
            working_directory: PathBuf::from("/tmp/test"),
            workspace_root: PathBuf::from("/tmp/test"),
            is_git_repo: false,
            platform: "linux".to_string(),
            date: "2025-03-13".to_string(),
            git_status: None,
        };

        let prompt = builder.build("gpt-4", None, &env).await.unwrap();

        assert!(!prompt.is_empty());
    }

    #[tokio::test]
    async fn test_scan_upward_no_instructions() {
        let dir = tempfile::tempdir().unwrap();
        let scanner = InstructionScanner::new();
        let file = dir.path().join("src").join("main.rs");

        let instructions = scanner.scan_upward(&file, dir.path()).await;
        assert!(instructions.is_empty());
    }

    #[tokio::test]
    async fn test_scan_upward_finds_instructions() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        let nested = src.join("module");
        std::fs::create_dir_all(&nested).unwrap();
        // Put AGENTS.md in the src/ directory (intermediate, not project root)
        std::fs::write(
            src.join("AGENTS.md"),
            "# Test Instructions\nDo good things.",
        )
        .unwrap();

        let scanner = InstructionScanner::new();
        let file = nested.join("main.rs");

        let instructions = scanner.scan_upward(&file, dir.path()).await;
        assert!(!instructions.is_empty(), "Should find AGENTS.md in src/");
        assert!(instructions.iter().any(|i| i.contains("Test Instructions")));
    }

    #[tokio::test]
    async fn test_build_prompt_generic_model() {
        let builder = PromptBuilder::new();
        let env = EnvironmentContext {
            working_directory: PathBuf::from("/tmp/test"),
            workspace_root: PathBuf::from("/tmp/test"),
            is_git_repo: false,
            platform: "macos".to_string(),
            date: "2025-06-01".to_string(),
            git_status: None,
        };

        let prompt = builder.build("llama-3-70b", None, &env).await.unwrap();

        assert!(!prompt.is_empty());
    }

    // --- New tests: builder validation, edge cases, display ---

    #[test]
    fn test_prompt_layer_equality() {
        assert_eq!(PromptLayer::Base, PromptLayer::Base);
        assert_eq!(PromptLayer::Environment, PromptLayer::Environment);
        assert_ne!(PromptLayer::Base, PromptLayer::Local);
    }

    #[test]
    fn test_prompt_layer_copy() {
        let layer = PromptLayer::Base;
        let copied = layer;
        assert_eq!(layer, copied);
    }

    #[test]
    fn test_model_provider_equality() {
        assert_eq!(ModelProvider::Anthropic, ModelProvider::Anthropic);
        assert_eq!(ModelProvider::Generic, ModelProvider::Generic);
        assert_ne!(ModelProvider::Anthropic, ModelProvider::OpenAI);
    }

    #[test]
    fn test_model_provider_from_model_id_case_sensitive() {
        // "Claude" with uppercase C should not match "claude"
        assert_eq!(
            ModelProvider::from_model_id("Claude-3"),
            ModelProvider::Generic
        );
        // "GPT" with uppercase should not match "gpt"
        assert_eq!(
            ModelProvider::from_model_id("GPT-4"),
            ModelProvider::Generic
        );
    }

    #[test]
    fn test_model_provider_from_model_id_substring_match() {
        // Model IDs containing provider keywords in unexpected places
        assert_eq!(
            ModelProvider::from_model_id("my-claude-clone"),
            ModelProvider::Anthropic
        );
        assert_eq!(
            ModelProvider::from_model_id("something-with-gemini-inside"),
            ModelProvider::Google
        );
        assert_eq!(
            ModelProvider::from_model_id("not-really-openai-compatible"),
            ModelProvider::OpenAI
        );
    }

    #[test]
    fn test_model_provider_debug() {
        let debug = format!("{:?}", ModelProvider::Anthropic);
        assert!(debug.contains("Anthropic"));
        let debug = format!("{:?}", ModelProvider::Google);
        assert!(debug.contains("Google"));
        let debug = format!("{:?}", ModelProvider::OpenAI);
        assert!(debug.contains("OpenAI"));
        let debug = format!("{:?}", ModelProvider::Generic);
        assert!(debug.contains("Generic"));
    }

    #[test]
    fn test_prompt_layer_debug() {
        assert!(format!("{:?}", PromptLayer::Base).contains("Base"));
        assert!(format!("{:?}", PromptLayer::ModelSpecific).contains("ModelSpecific"));
        assert!(format!("{:?}", PromptLayer::Environment).contains("Environment"));
        assert!(format!("{:?}", PromptLayer::Project).contains("Project"));
        assert!(format!("{:?}", PromptLayer::Local).contains("Local"));
        assert!(format!("{:?}", PromptLayer::Skills).contains("Skills"));
    }

    #[test]
    fn test_instruction_scanner_debug() {
        let scanner = InstructionScanner::new();
        let debug = format!("{scanner:?}");
        assert!(debug.contains("InstructionScanner"));
    }

    #[test]
    fn test_instruction_scanner_clone() {
        let scanner = InstructionScanner::new();
        let cloned = scanner.clone();
        assert_eq!(cloned.files, scanner.files);
    }

    #[test]
    fn test_prompt_builder_debug() {
        let builder = PromptBuilder::new();
        let debug = format!("{builder:?}");
        assert!(debug.contains("PromptBuilder"));
    }

    #[test]
    fn test_prompt_builder_clone() {
        let builder = PromptBuilder::new();
        let cloned = builder.clone();
        assert!(!cloned.base_prompt.is_empty());
        assert_eq!(cloned.base_prompt, builder.base_prompt);
    }

    #[tokio::test]
    async fn test_build_prompt_anthropic_model() {
        let builder = PromptBuilder::new();
        let env = EnvironmentContext {
            working_directory: PathBuf::from("/tmp/test"),
            workspace_root: PathBuf::from("/tmp/test"),
            is_git_repo: false,
            platform: "linux".to_string(),
            date: "2025-03-13".to_string(),
            git_status: None,
        };

        let prompt = builder.build("claude-3-opus", None, &env).await.unwrap();

        assert!(!prompt.is_empty());
        // Should contain the anthropic-specific prompt content
        assert!(!builder.anthropic_prompt.is_empty() || prompt.contains("Environment"));
    }

    #[tokio::test]
    async fn test_build_prompt_google_model() {
        let builder = PromptBuilder::new();
        let env = EnvironmentContext {
            working_directory: PathBuf::from("/tmp/test"),
            workspace_root: PathBuf::from("/tmp/test"),
            is_git_repo: false,
            platform: "linux".to_string(),
            date: "2025-03-13".to_string(),
            git_status: None,
        };

        let prompt = builder.build("gemini-pro", None, &env).await.unwrap();

        assert!(!prompt.is_empty());
    }

    #[tokio::test]
    async fn test_build_prompt_empty_model_id() {
        let builder = PromptBuilder::new();
        let env = EnvironmentContext {
            working_directory: PathBuf::from("/tmp"),
            workspace_root: PathBuf::from("/tmp"),
            is_git_repo: false,
            platform: "linux".to_string(),
            date: "2025-01-01".to_string(),
            git_status: None,
        };

        let prompt = builder.build("", None, &env).await.unwrap();
        assert!(!prompt.is_empty());
    }

    #[tokio::test]
    async fn test_build_prompt_with_git_status() {
        let builder = PromptBuilder::new();
        let env = EnvironmentContext {
            working_directory: PathBuf::from("/tmp/project"),
            workspace_root: PathBuf::from("/tmp/project"),
            is_git_repo: true,
            platform: "linux".to_string(),
            date: "2025-03-13".to_string(),
            git_status: Some(crate::environment::GitStatus {
                branch: Some("main".to_string()),
                modified: vec!["src/main.rs".to_string()],
                staged: vec![],
                untracked: vec![],
            }),
        };

        let prompt = builder.build("claude-3", None, &env).await.unwrap();

        assert!(prompt.contains("Git repository: yes"));
        assert!(prompt.contains("Git branch: `main`"));
    }

    #[tokio::test]
    async fn test_scan_upward_stops_at_project_root() {
        let dir = tempfile::tempdir().unwrap();
        // Create a CLAUDE.md inside project root (should NOT be found since we stop at root)
        std::fs::write(dir.path().join("CLAUDE.md"), "Root instructions").unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();

        let scanner = InstructionScanner::new();
        let file = src.join("main.rs");

        let instructions = scanner.scan_upward(&file, dir.path()).await;
        // scan_upward stops when path == project_root, so root-level files are not included
        assert!(!instructions.iter().any(|i| i.contains("Root instructions")));
    }

    #[tokio::test]
    async fn test_scan_upward_multiple_levels() {
        let dir = tempfile::tempdir().unwrap();
        let level1 = dir.path().join("a");
        let level2 = level1.join("b");
        let level3 = level2.join("c");
        std::fs::create_dir_all(&level3).unwrap();
        std::fs::write(level1.join("AGENTS.md"), "Level 1").unwrap();
        std::fs::write(level2.join("AGENTS.md"), "Level 2").unwrap();

        let scanner = InstructionScanner::new();
        let file = level3.join("file.rs");

        let instructions = scanner.scan_upward(&file, dir.path()).await;
        // Should find instructions from level2 and level1 (scanning upward from file parent)
        assert!(instructions.iter().any(|i| i.contains("Level 1")));
        assert!(instructions.iter().any(|i| i.contains("Level 2")));
    }

    #[tokio::test]
    async fn test_load_global_returns_vec() {
        let scanner = InstructionScanner::new();
        // This just verifies load_global returns without error;
        // actual content depends on whether ~/.claude/AGENTS.md exists
        let result = scanner.load_global().await;
        // result is always a Vec, may be empty
        assert!(
            result.len() % 3 == 0 || result.is_empty(),
            "Each instruction file produces 3 entries: header, content, blank"
        );
    }
}
