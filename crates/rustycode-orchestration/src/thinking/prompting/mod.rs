//! Strategy-specific prompt templates for LLM reasoning

use crate::thinking::core::error::Result;
use crate::thinking::core::types::Thought;
use async_trait::async_trait;
use std::collections::HashMap;

pub mod context;
pub mod templates;

pub use context::PromptContext;
pub use templates::PromptTemplateRegistry;

/// Base trait for rendering strategy-specific prompts
#[async_trait]
pub trait PromptTemplate: Send + Sync {
    /// Strategy name (Sequential, Dialectic, Parallel, Analogical, Abductive)
    fn strategy_name(&self) -> &'static str;

    /// Render the prompt with given context.
    ///
    fn render(&self, context: &PromptContext) -> Result<String>;

    /// Maximum tokens expected in response
    fn max_response_tokens(&self) -> u32 {
        2000 // Default, can be overridden
    }

    /// System role definition for this strategy
    fn system_role(&self) -> &'static str;
}

/// Strategy-specific prompt variants
#[derive(Debug, Clone)]
pub enum StrategyPrompt {
    Sequential { steps: usize },
    Dialectic { theme: String },
    Parallel { branches: usize },
    Analogical { domain_hint: Option<String> },
    Abductive { hypotheses_count: usize },
}

/// Context variables for prompt rendering
#[derive(Debug, Clone)]
pub struct PromptRenderContext {
    pub problem: String,
    pub previous_thoughts: Vec<Thought>,
    pub constraints: Vec<String>,
    pub current_depth: usize,
    pub iteration: usize,
    pub graph_summary: String,
    pub metadata: HashMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_context_creation() {
        let context = PromptContext::new("Test problem");
        assert_eq!(context.problem, "Test problem");
    }

    #[test]
    fn test_strategy_prompt_variants() {
        let seq = StrategyPrompt::Sequential { steps: 3 };
        let dia = StrategyPrompt::Dialectic {
            theme: "tradeoff".into(),
        };
        let par = StrategyPrompt::Parallel { branches: 4 };
        let ana = StrategyPrompt::Analogical {
            domain_hint: Some("cache".into()),
        };
        let abd = StrategyPrompt::Abductive {
            hypotheses_count: 5,
        };

        let _ = (seq, dia, par, ana, abd);
    }

    #[test]
    fn test_prompt_render_context_fields() {
        let ctx = PromptRenderContext {
            problem: "solve X".into(),
            previous_thoughts: vec![],
            constraints: vec!["no recursion".into()],
            current_depth: 2,
            iteration: 3,
            graph_summary: "5 nodes".into(),
            metadata: HashMap::new(),
        };
        assert_eq!(ctx.problem, "solve X");
        assert_eq!(ctx.constraints.len(), 1);
        assert_eq!(ctx.current_depth, 2);
    }

    #[test]
    fn test_template_registry_new() {
        let registry = PromptTemplateRegistry::new();
        // Should have default templates
        let _ = registry;
    }

    #[test]
    fn test_strategy_prompt_debug() {
        let seq = StrategyPrompt::Sequential { steps: 3 };
        let debug = format!("{seq:?}");
        assert!(debug.contains("Sequential"));
    }

    #[test]
    fn test_prompt_render_context_default_values() {
        let ctx = PromptRenderContext {
            problem: "test".into(),
            previous_thoughts: vec![],
            constraints: vec![],
            current_depth: 0,
            iteration: 0,
            graph_summary: String::new(),
            metadata: HashMap::new(),
        };
        assert!(ctx.previous_thoughts.is_empty());
        assert!(ctx.constraints.is_empty());
        assert!(ctx.metadata.is_empty());
    }

    #[test]
    fn test_prompt_template_default_max_tokens() {
        struct TestTemplate;
        #[async_trait::async_trait]
        impl PromptTemplate for TestTemplate {
            fn strategy_name(&self) -> &'static str {
                "Test"
            }
            fn render(&self, _context: &PromptContext) -> Result<String> {
                Ok("test".into())
            }
            fn system_role(&self) -> &'static str {
                "test"
            }
        }
        let t = TestTemplate;
        assert_eq!(t.max_response_tokens(), 2000);
    }

    #[test]
    fn test_strategy_prompt_all_variants() {
        let variants = [
            format!("{:?}", StrategyPrompt::Sequential { steps: 1 }),
            format!("{:?}", StrategyPrompt::Dialectic { theme: "x".into() }),
            format!("{:?}", StrategyPrompt::Parallel { branches: 2 }),
            format!("{:?}", StrategyPrompt::Analogical { domain_hint: None }),
            format!(
                "{:?}",
                StrategyPrompt::Abductive {
                    hypotheses_count: 3
                }
            ),
        ];
        assert_eq!(variants.len(), 5);
    }
}
