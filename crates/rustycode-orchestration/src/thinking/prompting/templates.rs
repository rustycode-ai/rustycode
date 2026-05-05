//! Prompt template implementations for each reasoning strategy

use super::{PromptContext, PromptTemplate};
use crate::thinking::core::error::{Error, Result};
use handlebars::Handlebars;
use serde_json::json;

/// Registry and factory for prompt templates
pub struct PromptTemplateRegistry {
    hb: Handlebars<'static>,
}

impl PromptTemplateRegistry {
    /// Register all built-in prompt templates.
    ///
    /// Each call uses `expect()` because these are compile-time constant strings
    /// that cannot fail at runtime. Clippy normally denies `expect_used` in this
    /// workspace, but panicking is the correct behaviour here: a malformed
    /// built-in template is a programming error, not a recoverable condition.
    #[allow(clippy::expect_used)]
    #[must_use]
    pub fn new() -> Self {
        let mut hb = Handlebars::new();

        hb.register_template_string("json_schema", Self::json_schema_partial())
            .expect("failed to register built-in template 'json_schema'");

        // Register all strategy templates
        hb.register_template_string("sequential", Self::sequential_template())
            .expect("failed to register built-in template 'sequential'");
        hb.register_template_string("dialectic", Self::dialectic_template())
            .expect("failed to register built-in template 'dialectic'");
        hb.register_template_string("parallel", Self::parallel_template())
            .expect("failed to register built-in template 'parallel'");
        hb.register_template_string("analogical", Self::analogical_template())
            .expect("failed to register built-in template 'analogical'");
        hb.register_template_string("abductive", Self::abductive_template())
            .expect("failed to register built-in template 'abductive'");
        hb.register_template_string("implementation", Self::implementation_template())
            .expect("failed to register built-in template 'implementation'");

        Self { hb }
    }

    /// Register a custom template at runtime.
    ///
    pub fn register_template(&mut self, name: &str, template: &str) -> Result<()> {
        self.hb
            .register_template_string(name, template)
            .map_err(|e| Error::ConfigError(format!("Template registration error: {e}")))
    }

    /// Render a strategy template with context.
    ///
    pub fn render(&self, strategy: &str, context: &PromptContext) -> Result<String> {
        let data = json!({
            "problem": context.problem,
            "thoughts_summary": context.thoughts_summary(250),
            "constraints": context.format_constraints(),
            "goal": context.format_goal(),
            "depth": context.current_depth,
            "iteration": context.iteration,
            "graph_summary": context.graph_summary,
        });

        self.hb
            .render(strategy, &data)
            .map_err(|e| Error::ConfigError(format!("Template render error: {e}")))
    }

    const fn json_schema_partial() -> &'static str {
        r#"Output as JSON with this exact structure:
{"thoughts": [{"kind": "Analysis", "content": "...", "confidence": 0.8, "reasoning": "..."}]}"#
    }

    const fn sequential_template() -> &'static str {
        r"
You are a methodical, step-by-step problem solver.

## Problem
{{problem}}
{{#if goal}}

{{goal}}
{{/if}}

## Current State
Depth: {{depth}}, Iteration: {{iteration}}
{{#if constraints}}
{{constraints}}
{{/if}}

## Previous Thoughts
{{thoughts_summary}}

## Task
Provide the next step in solving this problem. Be concrete, actionable, and specific.
Focus on moving toward a solution incrementally.

{{> json_schema}}
"
    }

    const fn dialectic_template() -> &'static str {
        r"
You are a careful thinker who considers multiple sides of issues.

## Problem
{{problem}}
{{#if goal}}

{{goal}}
{{/if}}

## Current State
Depth: {{depth}}, Iteration: {{iteration}}
{{#if constraints}}
{{constraints}}
{{/if}}

## Previous Thoughts
{{thoughts_summary}}

## Task
Provide either a thesis (main position), antithesis (opposing view), or synthesis (reconciliation).
Alternate between these perspectives to explore the problem from multiple angles.

{{> json_schema}}
"
    }

    const fn parallel_template() -> &'static str {
        r"
You are a systems thinker who considers multiple independent analyses.

## Problem
{{problem}}
{{#if goal}}

{{goal}}
{{/if}}

## Current State
Depth: {{depth}}, Iteration: {{iteration}}
{{#if constraints}}
{{constraints}}
{{/if}}

## Previous Thoughts
{{thoughts_summary}}

## Task
Provide 2-3 independent analyses or perspectives on this problem.
Each should explore a different aspect without relying on the others.

{{> json_schema}}
"
    }

    const fn analogical_template() -> &'static str {
        r"
You are a creative thinker who finds patterns and analogies.

## Problem
{{problem}}
{{#if goal}}

{{goal}}
{{/if}}

## Current State
Depth: {{depth}}, Iteration: {{iteration}}
{{#if constraints}}
{{constraints}}
{{/if}}

## Previous Thoughts
{{thoughts_summary}}

## Task
Find analogies from known domains and map them to this problem.
Think about similar challenges in other fields and how they were solved.

{{> json_schema}}
"
    }

    const fn abductive_template() -> &'static str {
        r"
You are a detective finding the best explanation for observations.

## Problem
{{problem}}
{{#if goal}}

{{goal}}
{{/if}}

## Current State
Depth: {{depth}}, Iteration: {{iteration}}
{{#if constraints}}
{{constraints}}
{{/if}}

## Previous Thoughts
{{thoughts_summary}}

## Task
Generate hypotheses that would explain key observations about this problem.
Evaluate which hypothesis is most likely and why.

        {{> json_schema}}
"
    }

    const fn implementation_template() -> &'static str {
        r#"You are a senior software engineer implementing production-quality code.

## Task
{{problem}}
{{#if goal}}

{{goal}}
{{/if}}

## Current State
Depth: {{depth}}, Iteration: {{iteration}}
{{#if constraints}}
{{constraints}}
{{/if}}

## Previous Work
{{thoughts_summary}}

## Instructions
First, analyze the problem and plan your approach. Then provide a complete, compilable implementation.
- Include all necessary imports and type definitions
- Handle edge cases and error conditions
- Write clear, idiomatic code with proper error handling
- Do NOT use pseudocode, TODO comments, or placeholder implementations

Output as JSON with this exact structure:
{"thoughts": [{"kind": "Analysis", "content": "your analysis and approach", "confidence": 0.9, "reasoning": "why this approach"}, {"kind": "Resolution", "content": "complete implementation code", "confidence": 0.9, "reasoning": "key design decisions"}]}
"#
    }
}

impl Default for PromptTemplateRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// Strategy-specific template implementations

pub struct SequentialTemplate;

impl PromptTemplate for SequentialTemplate {
    fn strategy_name(&self) -> &'static str {
        "Sequential"
    }

    fn render(&self, context: &PromptContext) -> Result<String> {
        let registry = PromptTemplateRegistry::new();
        registry.render("sequential", context)
    }

    fn system_role(&self) -> &'static str {
        "You are a methodical, step-by-step problem solver. Break problems into ordered steps."
    }
}

pub struct DialecticTemplate;

impl PromptTemplate for DialecticTemplate {
    fn strategy_name(&self) -> &'static str {
        "Dialectic"
    }

    fn render(&self, context: &PromptContext) -> Result<String> {
        let registry = PromptTemplateRegistry::new();
        registry.render("dialectic", context)
    }

    fn system_role(&self) -> &'static str {
        "You are a careful thinker. Consider multiple sides: thesis, antithesis, synthesis."
    }
}

pub struct ParallelTemplate;

impl PromptTemplate for ParallelTemplate {
    fn strategy_name(&self) -> &'static str {
        "Parallel"
    }

    fn render(&self, context: &PromptContext) -> Result<String> {
        let registry = PromptTemplateRegistry::new();
        registry.render("parallel", context)
    }

    fn system_role(&self) -> &'static str {
        "You are a systems thinker. Analyze multiple independent perspectives concurrently."
    }
}

pub struct AnalogicalTemplate;

impl PromptTemplate for AnalogicalTemplate {
    fn strategy_name(&self) -> &'static str {
        "Analogical"
    }

    fn render(&self, context: &PromptContext) -> Result<String> {
        let registry = PromptTemplateRegistry::new();
        registry.render("analogical", context)
    }

    fn system_role(&self) -> &'static str {
        "You are a creative thinker. Find analogies and patterns from other domains."
    }
}

pub struct AbductiveTemplate;

impl PromptTemplate for AbductiveTemplate {
    fn strategy_name(&self) -> &'static str {
        "Abductive"
    }

    fn render(&self, context: &PromptContext) -> Result<String> {
        let registry = PromptTemplateRegistry::new();
        registry.render("abductive", context)
    }

    fn system_role(&self) -> &'static str {
        "You are a detective. Generate and evaluate hypotheses to find best explanations."
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sequential_template_render() -> Result<()> {
        let context = PromptContext::new("Solve a step-by-step problem");
        let template = SequentialTemplate;
        let rendered = template.render(&context)?;

        assert!(rendered.contains("step-by-step"));
        assert!(rendered.contains("JSON"));
        Ok(())
    }

    #[test]
    fn test_all_templates_have_system_roles() {
        assert!(!SequentialTemplate.system_role().is_empty());
        assert!(!DialecticTemplate.system_role().is_empty());
        assert!(!ParallelTemplate.system_role().is_empty());
        assert!(!AnalogicalTemplate.system_role().is_empty());
        assert!(!AbductiveTemplate.system_role().is_empty());
    }

    #[test]
    fn test_template_registry() -> Result<()> {
        let registry = PromptTemplateRegistry::new();
        let context = PromptContext::new("Test problem");

        let sequential = registry.render("sequential", &context)?;
        let dialectic = registry.render("dialectic", &context)?;

        // Verify they're different
        assert_ne!(sequential, dialectic);
        assert!(sequential.contains("step"));
        assert!(dialectic.contains("thesis"));

        Ok(())
    }

    #[test]
    fn test_goal_rendered_in_template() -> Result<()> {
        let context = PromptContext::new("Test problem")
            .with_goal("Find the optimal solution")
            .with_success_criteria(vec!["Cost < 100".to_string()]);
        let registry = PromptTemplateRegistry::new();

        let rendered = registry.render("sequential", &context)?;
        assert!(rendered.contains("## Goal"), "Should contain goal section");
        assert!(
            rendered.contains("Find the optimal solution"),
            "Should contain goal text"
        );
        assert!(
            rendered.contains("Success Criteria"),
            "Should contain criteria"
        );
        Ok(())
    }

    #[test]
    fn test_no_goal_no_waste() -> Result<()> {
        let context = PromptContext::new("Test problem");
        let registry = PromptTemplateRegistry::new();

        let rendered = registry.render("sequential", &context)?;
        assert!(
            !rendered.contains("## Goal"),
            "Should NOT contain goal section when no goal set"
        );
        Ok(())
    }

    #[test]
    fn test_empty_constraints_omitted() -> Result<()> {
        let context = PromptContext::new("Test problem");
        let registry = PromptTemplateRegistry::new();

        let rendered = registry.render("sequential", &context)?;
        assert!(
            !rendered.contains("No constraints"),
            "Should NOT contain 'No constraints' text"
        );
        Ok(())
    }

    #[test]
    fn test_invalid_strategy_name() {
        let registry = PromptTemplateRegistry::new();
        let context = PromptContext::new("Test");
        let result = registry.render("nonexistent_strategy", &context);
        assert!(result.is_err(), "Should fail for unknown template");
    }

    #[test]
    fn test_dialectic_template_render() -> Result<()> {
        let context = PromptContext::new("Test dialectic");
        let template = DialecticTemplate;
        let rendered = template.render(&context)?;
        assert!(rendered.contains("thesis") || rendered.contains("antithesis"));
        Ok(())
    }

    #[test]
    fn test_parallel_template_render() -> Result<()> {
        let context = PromptContext::new("Test parallel");
        let template = ParallelTemplate;
        let rendered = template.render(&context)?;
        assert!(rendered.contains("independent") || rendered.contains("perspectives"));
        Ok(())
    }

    #[test]
    fn test_analogical_template_render() -> Result<()> {
        let context = PromptContext::new("Test analogical");
        let template = AnalogicalTemplate;
        let rendered = template.render(&context)?;
        assert!(rendered.contains("analogy") || rendered.contains("pattern"));
        Ok(())
    }

    #[test]
    fn test_abductive_template_render() -> Result<()> {
        let context = PromptContext::new("Test abductive");
        let template = AbductiveTemplate;
        let rendered = template.render(&context)?;
        assert!(rendered.contains("hypothesis") || rendered.contains("detective"));
        Ok(())
    }

    #[test]
    fn test_all_templates_different_content() -> Result<()> {
        let context = PromptContext::new("Test");
        let registry = PromptTemplateRegistry::new();

        let seq = registry.render("sequential", &context)?;
        let dia = registry.render("dialectic", &context)?;
        let par = registry.render("parallel", &context)?;
        let ana = registry.render("analogical", &context)?;
        let abd = registry.render("abductive", &context)?;

        // All should be unique
        let outputs = [&seq, &dia, &par, &ana, &abd];
        for i in 0..outputs.len() {
            for j in (i + 1)..outputs.len() {
                assert_ne!(
                    outputs[i], outputs[j],
                    "Templates {i} and {j} should differ"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn test_default_registry() {
        let registry = PromptTemplateRegistry::default();
        let context = PromptContext::new("Test");
        let result = registry.render("sequential", &context);
        assert!(result.is_ok());
    }
}
