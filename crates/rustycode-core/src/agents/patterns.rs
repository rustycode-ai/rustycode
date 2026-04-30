//! Agent workflow patterns
//!
//! This module implements various agent workflow patterns:
//!
//! - **Prompt Chaining**: Chain multiple LLM calls where each builds on the previous
//! - **Evaluator-Optimizer**: One agent evaluates work, another improves it iteratively
//! - **Routing**: Classify requests and route to appropriate handler

use serde::{Deserialize, Serialize};

/// Configuration for prompt chaining
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptChainConfig {
    /// Maximum number of chain iterations
    pub max_iterations: usize,

    /// Whether to include previous outputs in next prompt
    pub include_previous: bool,

    /// Separator between chain outputs
    pub separator: String,
}

impl Default for PromptChainConfig {
    fn default() -> Self {
        Self {
            max_iterations: 5,
            include_previous: true,
            separator: "\n---\n".to_string(),
        }
    }
}

/// Prompt chain pattern - chain multiple LLM calls
///
/// Each call builds on the previous output, enabling iterative refinement.
#[derive(Debug, Clone)]
pub struct PromptChain {
    config: PromptChainConfig,
    chain: Vec<String>,
}

impl PromptChain {
    /// Create a new prompt chain
    pub fn new(config: PromptChainConfig) -> Self {
        Self {
            config,
            chain: Vec::new(),
        }
    }

    /// Create with default configuration
    pub fn with_defaults() -> Self {
        Self::new(PromptChainConfig::default())
    }

    /// Add a prompt to the chain
    pub fn add(&mut self, prompt: String) {
        self.chain.push(prompt);
    }

    /// Build the next prompt in the chain
    ///
    /// This combines the base prompt with previous outputs
    pub fn build_next_prompt(&self, base: &str) -> String {
        if self.chain.is_empty() || !self.config.include_previous {
            return base.to_string();
        }

        let mut result = base.to_string();

        for (i, output) in self.chain.iter().enumerate() {
            result.push_str(&self.config.separator);
            result.push_str(&format!("Iteration {}:\n{}", i + 1, output));
        }

        result
    }

    /// Get the number of iterations completed
    pub fn iterations(&self) -> usize {
        self.chain.len()
    }

    /// Check if max iterations reached
    pub fn is_complete(&self) -> bool {
        self.chain.len() >= self.config.max_iterations
    }

    /// Reset the chain
    pub fn reset(&mut self) {
        self.chain.clear();
    }

    /// Get the current chain state
    pub fn chain(&self) -> &[String] {
        &self.chain
    }
}

/// Configuration for evaluator-optimizer pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluatorOptimizerConfig {
    /// Maximum optimization iterations
    pub max_iterations: usize,

    /// Quality threshold (0.0 - 1.0) to stop optimizing
    pub quality_threshold: f32,

    /// Whether to show evaluation feedback to optimizer
    pub show_feedback: bool,

    /// Temperature for evaluator (lower for more consistent evaluation)
    pub evaluator_temperature: f32,

    /// Temperature for optimizer (higher for more creative improvements)
    pub optimizer_temperature: f32,
}

impl Default for EvaluatorOptimizerConfig {
    fn default() -> Self {
        Self {
            max_iterations: 3,
            quality_threshold: 0.8,
            show_feedback: true,
            evaluator_temperature: 0.1,
            optimizer_temperature: 0.7,
        }
    }
}

/// Result from an evaluation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationResult {
    /// Quality score (0.0 - 1.0)
    pub quality_score: f32,

    /// Feedback on what to improve
    pub feedback: String,

    /// Specific issues found
    pub issues: Vec<String>,

    /// Whether the quality threshold was met
    pub passed: bool,
}

/// Evaluator-optimizer pattern
///
/// One agent (evaluator) assesses quality, another (optimizer) improves it.
/// This continues iteratively until quality threshold is met or max iterations reached.
#[derive(Debug, Clone)]
pub struct EvaluatorOptimizer {
    config: EvaluatorOptimizerConfig,
}

impl EvaluatorOptimizer {
    /// Create a new evaluator-optimizer
    pub fn new(config: EvaluatorOptimizerConfig) -> Self {
        Self { config }
    }

    /// Create with default configuration
    pub fn with_defaults() -> Self {
        Self::new(EvaluatorOptimizerConfig::default())
    }

    /// Run an optimization cycle
    ///
    /// Takes the current content, evaluates it, and returns evaluation results
    pub fn evaluate(&self, content: &str, _criteria: &str) -> EvaluationResult {
        // For now, return a simple evaluation
        // In a full implementation, this would call an LLM to evaluate

        let word_count = content.split_whitespace().count() as f32;

        // Simple heuristic: longer content with specific criteria gets better score
        let quality_score = if word_count > 10.0 {
            0.9
        } else if word_count > 5.0 {
            0.7
        } else {
            0.5
        };

        let passed = quality_score >= self.config.quality_threshold;

        EvaluationResult {
            quality_score,
            feedback: if passed {
                "Content meets quality standards.".to_string()
            } else {
                format!(
                    "Content needs improvement. Current score: {:.2}",
                    quality_score
                )
            },
            issues: if word_count < 5.0 {
                vec!["Content is too short".to_string()]
            } else {
                Vec::new()
            },
            passed,
        }
    }

    /// Optimize content based on feedback
    ///
    /// Takes content and evaluation feedback, returns improved version
    pub fn optimize(&self, content: &str, _feedback: &str) -> String {
        // For now, return a simple optimization
        // In a full implementation, this would call an LLM to optimize

        // Expand short content
        if content.split_whitespace().count() < 5 {
            format!("{} (expanded with more details and context)", content)
        } else {
            content.to_string()
        }
    }

    /// Run the full evaluator-optimizer cycle
    ///
    /// Returns the final optimized content and evaluation results
    pub async fn run(
        &self,
        initial_content: &str,
        criteria: &str,
    ) -> (String, Vec<EvaluationResult>) {
        let mut content = initial_content.to_string();
        let mut evaluations = Vec::new();

        for _ in 0..self.config.max_iterations {
            let evaluation = self.evaluate(&content, criteria);
            let passed = evaluation.passed;

            evaluations.push(evaluation.clone());

            if passed {
                break;
            }

            content = self.optimize(&content, &evaluation.feedback);
        }

        (content, evaluations)
    }
}

/// Router pattern - classify and route requests
///
/// Analyzes incoming requests and routes them to appropriate handlers.
#[derive(Debug, Clone)]
pub struct Router {
    routes: Vec<Route>,
}

/// A single route with pattern and handler
#[derive(Debug, Clone)]
pub struct Route {
    /// Name/ID of this route
    pub name: String,

    /// Keywords that trigger this route
    pub keywords: Vec<String>,

    /// Handler identifier (e.g., subagent ID, tool name)
    pub handler: String,

    /// Description of what this route handles
    pub description: String,
}

impl Router {
    /// Create a new router
    pub fn new() -> Self {
        Self { routes: Vec::new() }
    }

    /// Add a route
    pub fn add_route(&mut self, route: Route) {
        self.routes.push(route);
    }

    /// Route a request to the appropriate handler
    ///
    /// Returns the handler ID, or None if no match found
    pub fn route(&self, request: &str) -> Option<String> {
        let request_lower = request.to_lowercase();

        for route in &self.routes {
            for keyword in &route.keywords {
                if request_lower.contains(&keyword.to_lowercase()) {
                    return Some(route.handler.clone());
                }
            }
        }

        None
    }

    /// Create a default router with common routes
    pub fn with_defaults() -> Self {
        let mut router = Self::new();

        router.add_route(Route {
            name: "code".to_string(),
            keywords: vec![
                "write".into(),
                "implement".into(),
                "code".into(),
                "function".into(),
            ],
            handler: "coder".to_string(),
            description: "Code generation and implementation".to_string(),
        });

        router.add_route(Route {
            name: "debug".to_string(),
            keywords: vec!["bug".into(), "error".into(), "fix".into(), "debug".into()],
            handler: "debugger".to_string(),
            description: "Debugging and error fixing".to_string(),
        });

        router.add_route(Route {
            name: "review".to_string(),
            keywords: vec!["review".into(), "check".into(), "improve".into()],
            handler: "reviewer".to_string(),
            description: "Code review and improvement".to_string(),
        });

        router
    }
}

impl Default for Router {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_chain() {
        let mut chain = PromptChain::with_defaults();
        chain.add("First iteration".to_string());
        chain.add("Second iteration".to_string());

        assert_eq!(chain.iterations(), 2);
        assert!(!chain.is_complete());

        let next = chain.build_next_prompt("Base prompt");
        assert!(next.contains("First iteration"));
        assert!(next.contains("Second iteration"));
    }

    #[test]
    fn test_prompt_chain_max_iterations() {
        let mut chain = PromptChain::new(PromptChainConfig {
            max_iterations: 2,
            ..Default::default()
        });

        chain.add("First".to_string());
        chain.add("Second".to_string());
        assert!(chain.is_complete());
    }

    #[test]
    fn test_evaluator_optimizer() {
        let eo = EvaluatorOptimizer::with_defaults();

        let short_content = "Too short content.";
        let evaluation = eo.evaluate(short_content, "Should be longer");

        assert!(!evaluation.passed);
        assert!(evaluation.feedback.contains("score"));
    }

    #[tokio::test]
    async fn test_evaluator_optimizer_run() {
        let eo = EvaluatorOptimizer::new(EvaluatorOptimizerConfig {
            max_iterations: 2,
            quality_threshold: 0.8,
            ..Default::default()
        });

        let initial = "Too short content.";
        let (final_content, evaluations) = eo.run(initial, "Should be longer").await;

        // Should have run at least one evaluation
        assert!(!evaluations.is_empty());
        // Content should have been modified
        assert_ne!(final_content, initial);
        // Should contain "expanded" since it was too short
        assert!(final_content.contains("expanded"));
    }

    #[test]
    fn test_router() {
        let router = Router::with_defaults();

        // "Write a function" contains "write" (code route)
        assert_eq!(router.route("Write a function"), Some("coder".to_string()));

        // "Fix this bug" contains "bug" (debug route)
        assert_eq!(router.route("Fix this bug"), Some("debugger".to_string()));

        // "Review this code" contains both "code" (checked first) and "review"
        // So it matches the code route first
        assert_eq!(router.route("Review this code"), Some("coder".to_string()));

        // "Please review this" only contains "review"
        assert_eq!(
            router.route("Please review this"),
            Some("reviewer".to_string())
        );

        assert_eq!(router.route("Unknown request"), None);
    }
}
