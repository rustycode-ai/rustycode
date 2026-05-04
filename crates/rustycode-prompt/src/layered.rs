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
use std::collections::HashMap;
use std::path::Path;
use tokio::fs;

/// Prompt layer types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PromptLayer {
    Base,
    ModelSpecific,
    Infrastructure,
    Environment,
    Project,
    Local,
    Skills,
}

/// Model provider for model-specific prompts
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ModelProvider {
    // Anthropic
    ClaudeOpus,
    ClaudeSonnet,
    ClaudeHaiku,

    // OpenAI
    GPT5,
    GPT4,
    OpenAIReasoning,

    // Google
    Gemini3,
    Gemini2,

    // Other providers
    Mistral,
    DeepSeek,
    Llama,
    Qwen,
    Cohere,

    // Fallback
    Generic,
}

impl ModelProvider {
    pub fn from_model_id(model_id: &str) -> Self {
        let id = model_id.to_lowercase();

        // Anthropic — specific models first
        if id.contains("claude") {
            if id.contains("opus") {
                return Self::ClaudeOpus;
            }
            if id.contains("haiku") {
                return Self::ClaudeHaiku;
            }
            return Self::ClaudeSonnet;
        }

        // OpenAI — o-series before gpt
        if id.starts_with("o1") || id.starts_with("o3") || id.starts_with("o4") {
            return Self::OpenAIReasoning;
        }
        if id.contains("gpt-5") || id.contains("gpt5") {
            return Self::GPT5;
        }
        if id.contains("gpt-4")
            || id.contains("gpt4")
            || id.contains("gpt-3")
            || id.contains("chatgpt")
            || id.contains("gpt-4o")
        {
            return Self::GPT4;
        }

        // Google
        if id.contains("gemini-3") {
            return Self::Gemini3;
        }
        if id.contains("gemini") {
            return Self::Gemini2;
        }

        // Other providers
        if id.contains("mistral") || id.contains("codestral") {
            return Self::Mistral;
        }
        if id.contains("deepseek") {
            return Self::DeepSeek;
        }
        if id.contains("llama") {
            return Self::Llama;
        }
        if id.contains("qwen") {
            return Self::Qwen;
        }
        if id.contains("command-r") {
            return Self::Cohere;
        }

        // openrouter/unknown with openai keyword → GPT4 fallback
        if id.contains("openai") || id.contains("openrouter") {
            return Self::GPT4;
        }

        Self::Generic
    }

    /// Returns true if this is an Anthropic model (Claude family).
    pub const fn is_anthropic(&self) -> bool {
        matches!(
            self,
            Self::ClaudeOpus | Self::ClaudeSonnet | Self::ClaudeHaiku
        )
    }

    /// Returns true if this is an `OpenAI` model (`GPT` or o-series).
    pub const fn is_openai(&self) -> bool {
        matches!(self, Self::GPT5 | Self::GPT4 | Self::OpenAIReasoning)
    }

    /// Returns true if this is a Google model (Gemini family).
    pub const fn is_google(&self) -> bool {
        matches!(self, Self::Gemini3 | Self::Gemini2)
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
    model_prompts: HashMap<ModelProvider, String>,
    infrastructure_prompt: String,
    scanner: InstructionScanner,
}

impl PromptBuilder {
    pub fn new() -> Self {
        let mut model_prompts: HashMap<ModelProvider, String> = HashMap::new();

        // Anthropic
        model_prompts.insert(
            ModelProvider::ClaudeOpus,
            include_str!("../prompts/claude-opus.txt").to_string(),
        );
        model_prompts.insert(
            ModelProvider::ClaudeSonnet,
            include_str!("../prompts/claude-sonnet.txt").to_string(),
        );
        model_prompts.insert(
            ModelProvider::ClaudeHaiku,
            include_str!("../prompts/claude-haiku.txt").to_string(),
        );

        // OpenAI
        model_prompts.insert(
            ModelProvider::GPT5,
            include_str!("../prompts/gpt5.txt").to_string(),
        );
        model_prompts.insert(
            ModelProvider::GPT4,
            include_str!("../prompts/gpt4.txt").to_string(),
        );
        model_prompts.insert(
            ModelProvider::OpenAIReasoning,
            include_str!("../prompts/o-series.txt").to_string(),
        );

        // Google
        model_prompts.insert(
            ModelProvider::Gemini3,
            include_str!("../prompts/gemini3.txt").to_string(),
        );
        model_prompts.insert(
            ModelProvider::Gemini2,
            include_str!("../prompts/gemini2.txt").to_string(),
        );

        // Other providers
        model_prompts.insert(
            ModelProvider::Mistral,
            include_str!("../prompts/mistral.txt").to_string(),
        );
        model_prompts.insert(
            ModelProvider::DeepSeek,
            include_str!("../prompts/deepseek.txt").to_string(),
        );
        model_prompts.insert(
            ModelProvider::Llama,
            include_str!("../prompts/llama.txt").to_string(),
        );
        model_prompts.insert(
            ModelProvider::Qwen,
            include_str!("../prompts/qwen.txt").to_string(),
        );
        model_prompts.insert(
            ModelProvider::Cohere,
            include_str!("../prompts/cohere.txt").to_string(),
        );

        // Fallback
        model_prompts.insert(
            ModelProvider::Generic,
            include_str!("../prompts/generic.txt").to_string(),
        );

        Self {
            base_prompt: include_str!("../prompts/base.txt").to_string(),
            model_prompts,
            infrastructure_prompt: include_str!("../prompts/infrastructure.txt").to_string(),
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

        // Layer 3: Infrastructure (static, cache-stable)
        layers.push(self.infrastructure_prompt.trim().to_string());
        layers.push(String::new());

        // Layer 4: Environment
        layers.push(env.format_markdown());
        layers.push(String::new());

        // Layer 5: Domain context
        if let Ok(Some(domain_path)) = DomainContext::discover(&env.workspace_root) {
            if let Ok(domain) = DomainContext::load_from_file(&domain_path) {
                let domain_block = domain.format_markdown();
                if !domain_block.trim().is_empty() {
                    layers.push(domain_block);
                    layers.push(String::new());
                }
            }
        }

        // Layer 6: Project instructions
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

        // Layer 7: Global instructions
        let global_instructions = self.scanner.load_global().await;
        if !global_instructions.is_empty() {
            layers.extend(global_instructions);
            layers.push(String::new());
        }

        Ok(layers.join("\n\n"))
    }

    #[allow(clippy::trivially_copy_pass_by_ref, clippy::expect_used)]
    fn get_model_prompt(&self, provider: &ModelProvider) -> &str {
        self.model_prompts.get(provider).unwrap_or_else(|| {
            self.model_prompts
                .get(&ModelProvider::Generic)
                .expect("Generic prompt must exist")
        })
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

    // --- Build tests ---

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

    // --- Model detection tests ---

    #[test]
    fn test_model_provider_detection_basic() {
        assert_eq!(
            ModelProvider::from_model_id("claude-3-sonnet"),
            ModelProvider::ClaudeSonnet
        );
        assert_eq!(
            ModelProvider::from_model_id("gemini-pro"),
            ModelProvider::Gemini2
        );
        assert_eq!(ModelProvider::from_model_id("gpt-4"), ModelProvider::GPT4);
        assert_eq!(
            ModelProvider::from_model_id("unknown-model"),
            ModelProvider::Generic
        );
    }

    #[test]
    fn test_model_provider_claude_variants() {
        assert_eq!(
            ModelProvider::from_model_id("claude-opus-4"),
            ModelProvider::ClaudeOpus
        );
        assert_eq!(
            ModelProvider::from_model_id("claude-opus-4-7"),
            ModelProvider::ClaudeOpus
        );
        assert_eq!(
            ModelProvider::from_model_id("claude-haiku-4"),
            ModelProvider::ClaudeHaiku
        );
        assert_eq!(
            ModelProvider::from_model_id("claude-3-5-sonnet"),
            ModelProvider::ClaudeSonnet
        );
        assert_eq!(
            ModelProvider::from_model_id("claude"),
            ModelProvider::ClaudeSonnet
        );
    }

    #[test]
    fn test_model_provider_gemini_variants() {
        assert_eq!(
            ModelProvider::from_model_id("gemini-3-pro"),
            ModelProvider::Gemini3
        );
        assert_eq!(
            ModelProvider::from_model_id("gemini-ultra"),
            ModelProvider::Gemini2
        );
        assert_eq!(
            ModelProvider::from_model_id("gemini-pro"),
            ModelProvider::Gemini2
        );
    }

    #[test]
    fn test_model_provider_openai_variants() {
        assert_eq!(ModelProvider::from_model_id("gpt-5.5"), ModelProvider::GPT5);
        assert_eq!(
            ModelProvider::from_model_id("gpt-5-turbo"),
            ModelProvider::GPT5
        );
        assert_eq!(ModelProvider::from_model_id("gpt-4o"), ModelProvider::GPT4);
        assert_eq!(
            ModelProvider::from_model_id("gpt-3.5-turbo"),
            ModelProvider::GPT4
        );
        assert_eq!(
            ModelProvider::from_model_id("chatgpt-4o"),
            ModelProvider::GPT4
        );
    }

    #[test]
    fn test_model_provider_reasoning_variants() {
        assert_eq!(
            ModelProvider::from_model_id("o1-preview"),
            ModelProvider::OpenAIReasoning
        );
        assert_eq!(
            ModelProvider::from_model_id("o3-mini"),
            ModelProvider::OpenAIReasoning
        );
        assert_eq!(
            ModelProvider::from_model_id("o4-mini"),
            ModelProvider::OpenAIReasoning
        );
    }

    #[test]
    fn test_model_provider_other_providers() {
        assert_eq!(
            ModelProvider::from_model_id("mistral-large"),
            ModelProvider::Mistral
        );
        assert_eq!(
            ModelProvider::from_model_id("codestral-latest"),
            ModelProvider::Mistral
        );
        assert_eq!(
            ModelProvider::from_model_id("deepseek-v3"),
            ModelProvider::DeepSeek
        );
        assert_eq!(
            ModelProvider::from_model_id("llama-3-70b"),
            ModelProvider::Llama
        );
        assert_eq!(
            ModelProvider::from_model_id("qwen-2.5-coder"),
            ModelProvider::Qwen
        );
        assert_eq!(
            ModelProvider::from_model_id("command-r-plus"),
            ModelProvider::Cohere
        );
    }

    #[test]
    fn test_model_provider_generic_fallback() {
        assert_eq!(
            ModelProvider::from_model_id("grok-2"),
            ModelProvider::Generic
        );
        assert_eq!(
            ModelProvider::from_model_id("nova-pro"),
            ModelProvider::Generic
        );
        assert_eq!(ModelProvider::from_model_id(""), ModelProvider::Generic);
    }

    #[test]
    fn test_model_provider_openrouter() {
        assert_eq!(
            ModelProvider::from_model_id("openrouter/anthropic/claude-3"),
            ModelProvider::ClaudeSonnet
        );
        assert_eq!(
            ModelProvider::from_model_id("openrouter/meta-llama/llama-3"),
            ModelProvider::Llama
        );
        assert_eq!(
            ModelProvider::from_model_id("openrouter-gpt-4"),
            ModelProvider::GPT4
        );
    }

    #[test]
    fn test_model_provider_case_insensitive() {
        assert_eq!(
            ModelProvider::from_model_id("Claude-3"),
            ModelProvider::ClaudeSonnet
        );
        assert_eq!(ModelProvider::from_model_id("GPT-4"), ModelProvider::GPT4);
        assert_eq!(
            ModelProvider::from_model_id("Gemini-Pro"),
            ModelProvider::Gemini2
        );
        assert_eq!(
            ModelProvider::from_model_id("Mistral-Large"),
            ModelProvider::Mistral
        );
    }

    #[test]
    fn test_model_provider_substring_match() {
        assert_eq!(
            ModelProvider::from_model_id("my-claude-clone"),
            ModelProvider::ClaudeSonnet
        );
        assert_eq!(
            ModelProvider::from_model_id("something-with-gemini-inside"),
            ModelProvider::Gemini2
        );
        assert_eq!(
            ModelProvider::from_model_id("not-really-openai-compatible"),
            ModelProvider::GPT4
        );
    }

    // --- Prompt routing tests ---

    #[test]
    fn test_provider_routing_returns_correct_prompt() {
        let builder = PromptBuilder::new();

        let opus_prompt = builder.get_model_prompt(&ModelProvider::ClaudeOpus);
        assert!(
            opus_prompt.contains("Claude") || opus_prompt.contains("claude"),
            "Opus prompt should contain Claude marker"
        );

        let sonnet_prompt = builder.get_model_prompt(&ModelProvider::ClaudeSonnet);
        assert!(
            sonnet_prompt.contains("Claude") || sonnet_prompt.contains("claude"),
            "Sonnet prompt should contain Claude marker"
        );

        let haiku_prompt = builder.get_model_prompt(&ModelProvider::ClaudeHaiku);
        assert!(
            haiku_prompt.contains("Claude") || haiku_prompt.contains("claude"),
            "Haiku prompt should contain Claude marker"
        );

        let gpt5_prompt = builder.get_model_prompt(&ModelProvider::GPT5);
        assert!(
            gpt5_prompt.contains("developer"),
            "GPT5 prompt should contain developer marker"
        );

        let gpt4_prompt = builder.get_model_prompt(&ModelProvider::GPT4);
        assert!(
            gpt4_prompt.contains("developer"),
            "GPT4 prompt should contain developer marker"
        );

        let gemini3_prompt = builder.get_model_prompt(&ModelProvider::Gemini3);
        assert!(
            gemini3_prompt.contains("Gemini"),
            "Gemini 3 prompt should contain Gemini"
        );

        let gemini2_prompt = builder.get_model_prompt(&ModelProvider::Gemini2);
        assert!(
            gemini2_prompt.contains("Gemini"),
            "Gemini 2 prompt should contain Gemini"
        );

        let generic_prompt = builder.get_model_prompt(&ModelProvider::Generic);
        assert!(
            generic_prompt.contains("Core Principles"),
            "Generic prompt should contain core principles"
        );
    }

    #[test]
    fn test_provider_routing_all_variants_have_prompts() {
        let builder = PromptBuilder::new();
        let variants = [
            ModelProvider::ClaudeOpus,
            ModelProvider::ClaudeSonnet,
            ModelProvider::ClaudeHaiku,
            ModelProvider::GPT5,
            ModelProvider::GPT4,
            ModelProvider::OpenAIReasoning,
            ModelProvider::Gemini3,
            ModelProvider::Gemini2,
            ModelProvider::Mistral,
            ModelProvider::DeepSeek,
            ModelProvider::Llama,
            ModelProvider::Qwen,
            ModelProvider::Cohere,
            ModelProvider::Generic,
        ];

        for variant in &variants {
            let prompt = builder.get_model_prompt(variant);
            assert!(
                !prompt.is_empty(),
                "Prompt for {:?} should not be empty",
                variant
            );
        }
    }

    #[tokio::test]
    async fn test_built_prompt_contains_model_specific_content() {
        let builder = PromptBuilder::new();
        let env = EnvironmentContext {
            working_directory: PathBuf::from("/tmp/test"),
            workspace_root: PathBuf::from("/tmp/test"),
            is_git_repo: false,
            platform: "linux".to_string(),
            date: "2026-04-25".to_string(),
            git_status: None,
        };

        let gpt4_prompt = builder
            .build("gpt-4", None, &env)
            .await
            .expect("build failed");
        assert!(
            gpt4_prompt.contains("developer"),
            "GPT-4 built prompt should contain developer instructions"
        );

        let gemini_prompt = builder
            .build("gemini-pro", None, &env)
            .await
            .expect("build failed");
        assert!(
            gemini_prompt.contains("Gemini"),
            "Gemini built prompt should contain Gemini guidance"
        );

        let llama_prompt = builder
            .build("llama-3", None, &env)
            .await
            .expect("build failed");
        assert!(
            llama_prompt.contains("Llama"),
            "Llama built prompt should contain Llama guidance"
        );
    }

    // --- Type trait tests ---

    #[test]
    fn test_prompt_layer_variants() {
        assert!(matches!(PromptLayer::Base, PromptLayer::Base));
        assert!(matches!(
            PromptLayer::ModelSpecific,
            PromptLayer::ModelSpecific
        ));
        assert!(matches!(
            PromptLayer::Infrastructure,
            PromptLayer::Infrastructure
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

        let prompt = builder.build("grok-2", None, &env).await.unwrap();
        assert!(!prompt.is_empty());
        assert!(
            prompt.contains("Core Principles"),
            "Unknown model should get generic fallback"
        );
    }

    // --- Equality and trait tests ---

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
        assert_eq!(ModelProvider::ClaudeSonnet, ModelProvider::ClaudeSonnet);
        assert_eq!(ModelProvider::Generic, ModelProvider::Generic);
        assert_ne!(ModelProvider::ClaudeOpus, ModelProvider::ClaudeSonnet);
    }

    #[test]
    fn test_model_provider_debug() {
        assert!(format!("{:?}", ModelProvider::ClaudeOpus).contains("ClaudeOpus"));
        assert!(format!("{:?}", ModelProvider::ClaudeSonnet).contains("ClaudeSonnet"));
        assert!(format!("{:?}", ModelProvider::ClaudeHaiku).contains("ClaudeHaiku"));
        assert!(format!("{:?}", ModelProvider::GPT5).contains("GPT5"));
        assert!(format!("{:?}", ModelProvider::GPT4).contains("GPT4"));
        assert!(format!("{:?}", ModelProvider::OpenAIReasoning).contains("OpenAIReasoning"));
        assert!(format!("{:?}", ModelProvider::Gemini3).contains("Gemini3"));
        assert!(format!("{:?}", ModelProvider::Gemini2).contains("Gemini2"));
        assert!(format!("{:?}", ModelProvider::Mistral).contains("Mistral"));
        assert!(format!("{:?}", ModelProvider::DeepSeek).contains("DeepSeek"));
        assert!(format!("{:?}", ModelProvider::Llama).contains("Llama"));
        assert!(format!("{:?}", ModelProvider::Qwen).contains("Qwen"));
        assert!(format!("{:?}", ModelProvider::Cohere).contains("Cohere"));
        assert!(format!("{:?}", ModelProvider::Generic).contains("Generic"));
    }

    #[test]
    fn test_prompt_layer_debug() {
        assert!(format!("{:?}", PromptLayer::Base).contains("Base"));
        assert!(format!("{:?}", PromptLayer::ModelSpecific).contains("ModelSpecific"));
        assert!(format!("{:?}", PromptLayer::Infrastructure).contains("Infrastructure"));
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

    // --- Build integration tests ---

    #[tokio::test]
    async fn test_build_prompt_claude_opus() {
        let builder = PromptBuilder::new();
        let env = EnvironmentContext {
            working_directory: PathBuf::from("/tmp/test"),
            workspace_root: PathBuf::from("/tmp/test"),
            is_git_repo: false,
            platform: "linux".to_string(),
            date: "2025-03-13".to_string(),
            git_status: None,
        };

        let prompt = builder.build("claude-opus-4", None, &env).await.unwrap();
        assert!(!prompt.is_empty());
        assert!(prompt.contains("Environment"));
    }

    #[tokio::test]
    async fn test_build_prompt_gemini_model() {
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
        std::fs::write(dir.path().join("CLAUDE.md"), "Root instructions").unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();

        let scanner = InstructionScanner::new();
        let file = src.join("main.rs");

        let instructions = scanner.scan_upward(&file, dir.path()).await;
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
        assert!(instructions.iter().any(|i| i.contains("Level 1")));
        assert!(instructions.iter().any(|i| i.contains("Level 2")));
    }

    #[tokio::test]
    async fn test_load_global_returns_vec() {
        let scanner = InstructionScanner::new();
        let result = scanner.load_global().await;
        assert!(
            result.len() % 3 == 0 || result.is_empty(),
            "Each instruction file produces 3 entries: header, content, blank"
        );
    }

    #[tokio::test]
    async fn test_build_prompt_includes_infrastructure() {
        let builder = PromptBuilder::new();
        let env = EnvironmentContext {
            working_directory: PathBuf::from("/tmp/test"),
            workspace_root: PathBuf::from("/tmp/test"),
            is_git_repo: false,
            platform: "linux".to_string(),
            date: "2026-05-03".to_string(),
            git_status: None,
        };

        let prompt = builder.build("claude-3", None, &env).await.unwrap();

        assert!(prompt.contains("Framework Capabilities"));
        assert!(prompt.contains("Tool Profiles"));
        assert!(prompt.contains("Execution Strategies"));
    }

    #[tokio::test]
    async fn test_infrastructure_appears_before_environment() {
        let builder = PromptBuilder::new();
        let env = EnvironmentContext {
            working_directory: PathBuf::from("/tmp/test"),
            workspace_root: PathBuf::from("/tmp/test"),
            is_git_repo: false,
            platform: "linux".to_string(),
            date: "2026-05-03".to_string(),
            git_status: None,
        };

        let prompt = builder.build("claude-3", None, &env).await.unwrap();

        let infra_pos = prompt
            .find("Framework Capabilities")
            .expect("missing infrastructure");
        let env_pos = prompt.find("## Environment").expect("missing environment");
        assert!(
            infra_pos < env_pos,
            "infrastructure must appear before environment"
        );
    }

    #[test]
    fn test_prompt_layer_infrastructure_variant() {
        assert!(matches!(
            PromptLayer::Infrastructure,
            PromptLayer::Infrastructure
        ));
        assert!(format!("{:?}", PromptLayer::Infrastructure).contains("Infrastructure"));
    }
}
