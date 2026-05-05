//! Integration tests for orchestration optimizations.
//!
//! Tests the interaction between parallel execution, caching, summarization,
//! tiered routing, and streaming to ensure they work together correctly.

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod integration_tests {
    use rustycode_orchestration::{
        cache::PromptCacheManager,
        config::{OrchestrationConfig, ParallelExecutionConfig, PromptCachingConfig},
        optimization_metrics::OptimizationMetrics,
        routing::{ComplexityClassifier, ModelRouter, RoutingPolicy, TaskComplexity},
        summary::{ResultSummarizer, SummaryConfig},
    };
    use std::fmt::Write;

    #[test]
    fn test_all_optimizations_enabled() {
        let config = OrchestrationConfig {
            parallel_execution: ParallelExecutionConfig {
                enabled: true,
                max_concurrent: 3,
            },
            prompt_caching: PromptCachingConfig {
                enabled: true,
                cache_system_prompt: true,
                cache_tool_definitions: true,
            },
            streaming_results: true,
            ..Default::default()
        };

        // Verify all components are configured
        assert!(config.parallel_execution.enabled);
        assert_eq!(config.parallel_execution.max_concurrent, 3);
        assert!(config.prompt_caching.enabled);
        assert!(config.prompt_caching.cache_system_prompt);
        assert!(config.prompt_caching.cache_tool_definitions);
        assert!(config.streaming_results);
    }

    #[test]
    fn test_cache_and_metrics_integration() {
        let mut cache = PromptCacheManager::new();
        let system_prompt = "You are a helpful assistant.";

        // Cache the prompt
        cache.cache_system_prompt(system_prompt);
        assert!(cache.is_system_prompt_cached());

        // Track in metrics
        let mut metrics = OptimizationMetrics::new();
        let tokens_in_cache = cache.estimate_tokens(system_prompt);
        metrics.record_cache_result(true, tokens_in_cache);

        assert_eq!(metrics.cache_hits, 1);
        assert_eq!(metrics.cache_hit_tokens, tokens_in_cache);
        assert!(metrics.cache_hit_rate > 0.0);
    }

    #[test]
    #[allow(clippy::format_push_string)]
    fn test_summarization_reduces_tokens() {
        let summarizer = ResultSummarizer::new(SummaryConfig::default());

        // Input must exceed max_output_chars (2000) to trigger summarization
        let mut original = String::from("key data: value\n");
        for i in 0..200 {
            writeln!(original, "[DEBUG] processing step {i} of 100").unwrap();
        }
        original.push_str("[ERROR] unexpected error occurred\n");
        original.push_str("Final result: success\n");

        let summary = summarizer
            .summarize("bash", &original)
            .expect("summarization should succeed");

        let original_tokens = summarizer.estimate_tokens(&original);
        let summary_tokens = summarizer.estimate_tokens(&summary);

        // Summarization should reduce tokens
        assert!(summary_tokens < original_tokens);
        assert!(summary.contains("ERROR"));
        assert!(summary.contains("success"));
    }

    #[test]
    fn test_complexity_classification_for_routing() {
        let classifier = ComplexityClassifier::default();
        let router = ModelRouter::new(RoutingPolicy::default());

        // Simple task
        let simple_task = rustycode_orchestration::routing::TaskDescriptor {
            description: "List files".to_string(),
            context: "minimal".to_string(),
            step_count: 1,
        };

        let simple_complexity = classifier.classify(&simple_task);
        assert_eq!(simple_complexity, TaskComplexity::Simple);
        let simple_tier = router.route(&simple_task);
        // Should route to fast tier (Musician tier 2)
        assert_ne!(
            simple_tier,
            rustycode_orchestration::types::ExecutionTier::Composer
        );

        // Complex task
        let complex_task = rustycode_orchestration::routing::TaskDescriptor {
            description: "Design and implement distributed consensus algorithm with Byzantine fault tolerance"
                .to_string(),
            context: "complex architecture decision required, multiple design options, 20+ implementation steps"
                .to_string(),
            step_count: 25,
        };

        let complex_complexity = classifier.classify(&complex_task);
        assert_eq!(complex_complexity, TaskComplexity::Complex);
        let complex_tier = router.route(&complex_task);
        // Should route to expensive tier (Composer tier 4)
        assert_eq!(
            complex_tier,
            rustycode_orchestration::types::ExecutionTier::Composer
        );
    }

    #[test]
    fn test_metrics_accumulation() {
        let mut metrics = OptimizationMetrics::new();

        // Record multiple optimizations
        metrics.record_execution(500, 200);
        metrics.record_summarization(1000, 600);
        metrics.record_cache_result(true, 100);
        metrics.record_cache_result(false, 0);

        // Verify accumulation
        assert_eq!(metrics.sequential_execution_time_ms, 500);
        assert_eq!(metrics.parallel_execution_time_ms, 200);
        assert_eq!(metrics.total_input_tokens, 1000);
        assert_eq!(metrics.summarized_input_tokens, 600);
        assert_eq!(metrics.tokens_saved_by_summarization, 400);
        assert_eq!(metrics.cache_hits, 1);
        assert_eq!(metrics.cache_misses, 1);

        // Compute rates
        metrics.compute_cache_hit_rate();
        assert!(metrics.cache_hit_rate > 0.4 && metrics.cache_hit_rate < 0.6);

        // Verify savings calculations
        assert!(metrics.time_savings_percent() > 50.0);
        assert!(metrics.token_savings_percent() > 40.0);
    }

    #[test]
    fn test_optimization_report_generation() {
        let mut metrics = OptimizationMetrics::new();

        // Simulate a realistic optimization scenario
        metrics.record_execution(5000, 1500); // 70% time savings
        metrics.record_summarization(100_000, 60_000); // 40% token reduction
        metrics.record_cache_result(true, 15_000);
        metrics.record_cache_result(true, 12_000);
        metrics.record_cache_result(false, 0);

        metrics.record_model_call(rustycode_orchestration::types::ExecutionTier::Musician);
        metrics.record_model_call(rustycode_orchestration::types::ExecutionTier::Musician);
        metrics.record_model_call(rustycode_orchestration::types::ExecutionTier::Editor);

        let report = metrics.report();

        // Verify report contains key metrics
        assert!(report.contains("Optimization Metrics Report"));
        assert!(report.contains("Time saved:          70.0%"));
        assert!(report.contains("Total tokens saved"));
        assert!(report.contains("Hit rate:"));
        assert!(report.contains("Musician calls: 2"));
        assert!(report.contains("Editor calls:   1"));
    }

    #[test]
    fn test_config_serialization() {
        let config = OrchestrationConfig::default();

        let json = serde_json::to_string(&config).expect("serialization failed");
        let deserialized: OrchestrationConfig =
            serde_json::from_str(&json).expect("deserialization failed");

        assert_eq!(
            deserialized.parallel_execution.enabled,
            config.parallel_execution.enabled
        );
        assert_eq!(
            deserialized.parallel_execution.max_concurrent,
            config.parallel_execution.max_concurrent
        );
        assert_eq!(deserialized.streaming_results, config.streaming_results);
    }

    #[test]
    fn test_cache_metrics_accuracy() {
        let mut cache = PromptCacheManager::new();

        // Cache multiple items
        cache.cache_system_prompt("System prompt content");
        cache.cache_tool_definitions(&["tool1", "tool2", "tool3"]);

        // Verify caching
        assert!(cache.is_system_prompt_cached());
        assert!(cache.is_tool_defs_cached());
        assert_eq!(cache.cached_tool_count(), 3);

        // Total cached tokens should be reasonable
        let total_tokens = cache.total_cached_tokens();
        assert!(total_tokens > 0);

        // Metrics should track accurately
        let mut metrics = OptimizationMetrics::new();
        let system_prompt_tokens = cache.estimate_tokens("System prompt content");
        metrics.record_cache_result(true, system_prompt_tokens);

        assert_eq!(metrics.cache_hits, 1);
        assert_eq!(metrics.cache_hit_tokens, system_prompt_tokens);
    }
}
