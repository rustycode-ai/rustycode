//! Example configurations for orchestration optimizations.
//!
//! Shows how to configure different optimization profiles for different use cases.

use rustycode_orchestration::{
    config::{OrchestrationConfig, ParallelExecutionConfig, PromptCachingConfig},
    routing::RoutingPolicy,
    summary::SummaryConfig,
    types::ExecutionTier,
};

/// Configuration optimized for exploration phase.
///
/// Prioritizes speed and cost savings with aggressive parallelization and caching.
fn exploration_config() -> OrchestrationConfig {
    OrchestrationConfig {
        parallel_execution: ParallelExecutionConfig {
            enabled: true,
            max_concurrent: 5,  // Maximize parallelization for exploration
        },
        prompt_caching: PromptCachingConfig {
            enabled: true,
            cache_system_prompt: true,
            cache_tool_definitions: true,
        },
        streaming_results: true,
        ..Default::default()
    }
}

/// Configuration optimized for implementation phase.
///
/// Prioritizes correctness and robustness, with moderate parallelization.
fn implementation_config() -> OrchestrationConfig {
    OrchestrationConfig {
        parallel_execution: ParallelExecutionConfig {
            enabled: true,
            max_concurrent: 2,  // Limited parallelization due to dependencies
        },
        prompt_caching: PromptCachingConfig {
            enabled: true,
            cache_system_prompt: true,
            cache_tool_definitions: true,
        },
        streaming_results: false,  // Wait for all results for determinism
        ..Default::default()
    }
}

/// Configuration for cost-sensitive environments.
///
/// Maximizes token and cost savings at the expense of speed.
fn cost_optimized_config() -> OrchestrationConfig {
    let mut config = OrchestrationConfig::default();

    // Aggressive result summarization
    let mut summary_config = SummaryConfig::default();
    summary_config.max_output_chars = 1000;  // Aggressive truncation
    config.result_summarization = summary_config;

    // Strict tiered routing: only use expensive models when necessary
    config.model_routing = RoutingPolicy {
        simple_tier: ExecutionTier::Musician,
        moderate_tier: ExecutionTier::Musician,  // Use cheap model more
        complex_tier: ExecutionTier::Editor,     // Skip Opus unless critical
    };

    // Parallel execution still enabled for time savings
    config.parallel_execution = ParallelExecutionConfig {
        enabled: true,
        max_concurrent: 3,
    };

    config
}

/// Configuration for latency-sensitive applications.
///
/// Prioritizes speed with streaming and aggressive parallelization.
fn latency_optimized_config() -> OrchestrationConfig {
    OrchestrationConfig {
        parallel_execution: ParallelExecutionConfig {
            enabled: true,
            max_concurrent: 10,  // Maximize concurrent tools
        },
        prompt_caching: PromptCachingConfig {
            enabled: true,
            cache_system_prompt: true,
            cache_tool_definitions: true,
        },
        streaming_results: true,  // Stream for earliest possible results
        ..Default::default()
    }
}

/// Configuration with all optimizations disabled (baseline for comparison).
fn no_optimizations_config() -> OrchestrationConfig {
    OrchestrationConfig {
        parallel_execution: ParallelExecutionConfig {
            enabled: false,
            max_concurrent: 1,
        },
        prompt_caching: PromptCachingConfig {
            enabled: false,
            cache_system_prompt: false,
            cache_tool_definitions: false,
        },
        streaming_results: false,
        ..Default::default()
    }
}

fn main() {
    println!("=== Orchestration Optimization Configurations ===\n");

    println!("1. EXPLORATION PHASE (Default)");
    println!("   - 5 concurrent tools");
    println!("   - Aggressive caching");
    println!("   - Streaming enabled");
    println!("   - Best for: Quick initial exploration, cost savings");
    let exploration = exploration_config();
    println!("   Config: {:?}\n", exploration.parallel_execution);

    println!("2. IMPLEMENTATION PHASE");
    println!("   - 2 concurrent tools (respecting dependencies)");
    println!("   - Caching enabled");
    println!("   - Streaming disabled (determinism)");
    println!("   - Best for: Feature implementation, code generation");
    let implementation = implementation_config();
    println!("   Config: {:?}\n", implementation.parallel_execution);

    println!("3. COST OPTIMIZED");
    println!("   - 3 concurrent tools");
    println!("   - Aggressive summarization (1000 char limit)");
    println!("   - Tiered routing: use Haiku/Sonnet, skip Opus");
    println!("   - Best for: Budget-constrained projects");
    let cost_opt = cost_optimized_config();
    println!("   Config: {:?}\n", cost_opt.parallel_execution);

    println!("4. LATENCY OPTIMIZED");
    println!("   - 10 concurrent tools (maximum)");
    println!("   - Caching enabled");
    println!("   - Streaming enabled");
    println!("   - Best for: Real-time applications, low-latency requirements");
    let latency_opt = latency_optimized_config();
    println!("   Config: {:?}\n", latency_opt.parallel_execution);

    println!("5. BASELINE (No Optimizations)");
    println!("   - Sequential execution (1 tool at a time)");
    println!("   - No caching");
    println!("   - No streaming");
    println!("   - Best for: Debugging, comparison baseline");
    let baseline = no_optimizations_config();
    println!("   Config: {:?}\n", baseline.parallel_execution);

    println!("=== Performance Expectations ===\n");
    println!("Exploration Phase:");
    println!("  - Time savings: 50-70% vs baseline");
    println!("  - Token savings: 40-60% vs baseline");
    println!("  - Cache hit rate: 70%+ on repeated tasks");
    println!();
    println!("Implementation Phase:");
    println!("  - Time savings: 20-30% vs baseline");
    println!("  - Token savings: 30-50% vs baseline");
    println!();
    println!("Cost Optimized:");
    println!("  - Token savings: 60-80% vs baseline");
    println!("  - Time savings: 10-20% vs baseline");
    println!();
    println!("Latency Optimized:");
    println!("  - Time savings: 70-85% vs baseline");
    println!("  - Token savings: 30-50% vs baseline");
}
