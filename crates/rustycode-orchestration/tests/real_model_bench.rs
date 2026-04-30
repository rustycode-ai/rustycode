//! Real-model Terminal Bench — tests orchestration pipeline against live LLM models.
//!
//! Reads ~/.rustycode/config.json for API keys. Uses:
//!   - Z.AI endpoint directly for GLM models (openai-compatible provider)
//!   - OpenRouter for cross-provider free models
//!
//! Run: cargo test -p rustycode-orchestration --test real_model_bench -- --nocapture --test-threads=1

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::doc_markdown,
    clippy::let_and_return,
    clippy::format_push_string,
    clippy::redundant_clone,
    clippy::match_single_binding,
    clippy::bool_to_int_with_if,
    clippy::unnecessary_lazy_evaluations,
    clippy::manual_let_else,
    clippy::collapsible_if,
    clippy::useless_conversion,
    clippy::cast_lossless,
    clippy::len_zero,
    clippy::match_overlapping_arm,
    clippy::explicit_auto_deref,
    clippy::uninlined_format_args,
    clippy::single_match_else,
    clippy::needless_borrow,
    clippy::option_if_let_else,
    unused_parens
)]

use rustycode_llm::provider::{ChatMessage, CompletionRequest, LLMProvider, ProviderConfig};
use rustycode_orchestration::quality_detector::QualityDetector;
use rustycode_orchestration::strategy_selector::StrategySelector;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
struct AppConfig {
    providers: ProvidersConfig,
}

#[derive(Deserialize)]
struct ProvidersConfig {
    openai: ProviderCreds,
    openrouter: ProviderCreds,
}

#[derive(Deserialize)]
struct ProviderCreds {
    api_key: String,
    #[serde(default)]
    base_url: Option<String>,
}

#[derive(Clone)]
struct BenchModel {
    id: &'static str,
    display: &'static str,
    tier: &'static str,
    route: ProviderRoute,
}

#[derive(Clone, Copy)]
enum ProviderRoute {
    ZaiDirect,
    OpenRouter,
}

const ZAI_MODELS: &[BenchModel] = &[
    BenchModel {
        id: "glm-5.1",
        display: "GLM 5.1",
        tier: "flagship",
        route: ProviderRoute::ZaiDirect,
    },
    BenchModel {
        id: "glm-5-turbo",
        display: "GLM 5 Turbo",
        tier: "fast",
        route: ProviderRoute::ZaiDirect,
    },
    BenchModel {
        id: "glm-4.7-flash",
        display: "GLM 4.7 Flash",
        tier: "budget",
        route: ProviderRoute::ZaiDirect,
    },
];

const FREE_MODELS: &[BenchModel] = &[
    BenchModel {
        id: "z-ai/glm-4.5-air-free",
        display: "GLM 4.5 Air (free)",
        tier: "free",
        route: ProviderRoute::OpenRouter,
    },
    BenchModel {
        id: "inclusionai/ling-2.6-1t:free",
        display: "Ling 2.6 1T (free)",
        tier: "free",
        route: ProviderRoute::OpenRouter,
    },
];

const BENCH_TASKS: &[(&str, &str)] = &[
    ("mips-interpreter", "Implement a MIPS interpreter supporting R-type, I-type, and J-type instructions with register file, memory, and branch handling"),
    ("fix-typo", "Fix the typo on line 42 of src/main.rs where 'receieve' should be 'receive'"),
    ("refactor-auth", "Refactor the authentication module to use JWT tokens instead of session cookies, update middleware, maintain backward compatibility"),
    ("binary-search", "Implement binary search for a sorted array of integers with proper edge case handling"),
    ("explore-perf", "Analyze why the vector search in memory.rs is slow and propose optimization strategies"),
];

fn truncate_safe(s: &str, max_chars: usize) -> String {
    match s.char_indices().nth(max_chars) {
        Some((idx, _)) => format!("{}...", &s[..idx]),
        None => s.to_string(),
    }
}

fn load_config() -> Option<AppConfig> {
    let home = std::env::var("HOME").ok()?;
    let path = std::path::Path::new(&home).join(".rustycode/config.json");
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

fn build_zai_provider(model_id: &str, api_key: &str, base_url: &str) -> Arc<dyn LLMProvider> {
    std::env::set_var("OPENAI_API_KEY", api_key);
    let config = ProviderConfig {
        base_url: Some(base_url.to_string()),
        timeout_seconds: Some(120),
        ..ProviderConfig::default()
    };
    rustycode_llm::create_provider_with_config("openai", model_id, config)
        .expect("Failed to create Z.AI provider")
}

fn build_openrouter_provider(model_id: &str, api_key: &str) -> Arc<dyn LLMProvider> {
    std::env::set_var("OPENROUTER_API_KEY", api_key);
    rustycode_llm::create_provider("openrouter", model_id)
        .expect("Failed to create OpenRouter provider")
}

fn build_provider_for(model: &BenchModel, config: &AppConfig) -> Arc<dyn LLMProvider> {
    match model.route {
        ProviderRoute::ZaiDirect => {
            let base_url = config
                .providers
                .openai
                .base_url
                .as_deref()
                .unwrap_or("https://api.z.ai/api/coding/paas/v4");
            build_zai_provider(model.id, &config.providers.openai.api_key, base_url)
        }
        ProviderRoute::OpenRouter => {
            build_openrouter_provider(model.id, &config.providers.openrouter.api_key)
        }
    }
}

#[allow(clippy::too_many_lines)]
#[tokio::test]
async fn test_real_model_orchestration_bench() {
    let config = match load_config() {
        Some(c) => c,
        None => {
            eprintln!("SKIP: No config found at ~/.rustycode/config.json");
            return;
        }
    };

    let all_models: Vec<&BenchModel> = ZAI_MODELS.iter().chain(FREE_MODELS.iter()).collect();

    println!(
        "\n🧪 REAL MODEL ORCHESTRATION BENCH ({} models x {} tasks)",
        all_models.len(),
        BENCH_TASKS.len()
    );
    println!("{}\n", "=".repeat(60));

    let detector = QualityDetector::new();
    let mut results: Vec<RunResult> = Vec::new();

    for model in &all_models {
        let route_label = match model.route {
            ProviderRoute::ZaiDirect => "z.ai",
            ProviderRoute::OpenRouter => "openrouter",
        };
        println!(
            "📦 {} ({}) — tier: {} via {}",
            model.display, model.id, model.tier, route_label
        );

        let provider = build_provider_for(model, &config);

        for &(task_id, task_desc) in BENCH_TASKS {
            let complexity = StrategySelector::detect_complexity(task_desc);
            let quality_before = detector.evaluate(task_desc);
            let selector = StrategySelector::new();
            let strategy = selector.select(complexity, &quality_before, 75);

            println!("  [{task_id}] complexity={complexity:.2} strategy={strategy:?}...");

            let llm_response = call_llm(&provider, model.id, task_desc).await;
            let quality_after = detector.evaluate(&llm_response);
            let quality_delta = quality_after.total - quality_before.total;

            let is_error = llm_response.starts_with("ERROR:");
            let result = RunResult {
                model: model.display.to_string(),
                task: task_id.to_string(),
                complexity,
                strategy: format!("{strategy:?}"),
                quality_before: quality_before.total,
                quality_after: quality_after.total,
                quality_delta,
                response_len: llm_response.len(),
                snippet: truncate_safe(&llm_response, 100),
                is_error,
            };

            let status = if is_error {
                "❌"
            } else if quality_delta > 0.0 {
                "✅"
            } else {
                "⚠️"
            };
            println!(
                "    {} quality {:.2}→{:.2} (Δ{:+.2}) len={}",
                status,
                result.quality_before,
                result.quality_after,
                result.quality_delta,
                result.response_len
            );

            results.push(result);
        }
        println!();
    }

    println!("\n📈 SUMMARY");
    println!("{}", "=".repeat(60));
    for model in &all_models {
        let model_results: Vec<_> = results
            .iter()
            .filter(|r| r.model == model.display)
            .collect();
        if model_results.is_empty() {
            continue;
        }
        let errors = model_results.iter().filter(|r| r.is_error).count();
        let successes: Vec<_> = model_results.iter().filter(|r| !r.is_error).collect();
        let avg_delta: f64 = if successes.is_empty() {
            0.0
        } else {
            successes.iter().map(|r| r.quality_delta).sum::<f64>() / successes.len() as f64
        };
        let avg_len: f64 = if successes.is_empty() {
            0.0
        } else {
            successes.iter().map(|r| r.response_len as f64).sum::<f64>() / successes.len() as f64
        };
        println!("  {} ({})", model.display, model.tier);
        println!(
            "    ok={} err={} avg_delta={avg_delta:+.2} avg_len={avg_len:.0}",
            successes.len(),
            errors
        );
    }

    let total_errors = results.iter().filter(|r| r.is_error).count();
    assert!(
        total_errors < results.len(),
        "At least some tasks should succeed"
    );

    let non_error_delta: f64 = results
        .iter()
        .filter(|r| !r.is_error)
        .map(|r| r.quality_delta)
        .sum::<f64>();
    assert!(
        non_error_delta > 0.0,
        "Quality should improve on average across successful runs"
    );
}

#[tokio::test]
async fn test_zai_direct_deep_dive() {
    let config = match load_config() {
        Some(c) => c,
        None => {
            eprintln!("SKIP: No config");
            return;
        }
    };

    let model_id = "glm-4.7-flash";
    let base_url = config
        .providers
        .openai
        .base_url
        .as_deref()
        .unwrap_or("https://api.z.ai/api/coding/paas/v4");
    let provider = build_zai_provider(model_id, &config.providers.openai.api_key, base_url);

    println!("\n🔬 DEEP DIVE: z.ai/{model_id} — Structured Thinking Quality");

    let task = "Implement a MIPS interpreter supporting R-type, I-type, and J-type instructions. \
                Include register file (32 registers), byte-addressable memory, and a fetch-decode-execute cycle.";

    let prompt = format!(
        "You are solving a complex engineering task. Think step by step.\n\n\
         Task: {task}\n\n\
         Provide:\n\
         1. Architecture decision (what components)\n\
         2. Key design constraints (instruction formats, edge cases)\n\
         3. Implementation plan (phases)\n\
         4. Confidence level (0-100)"
    );

    let response = call_llm(&provider, model_id, &prompt).await;
    let detector = QualityDetector::new();
    let quality = detector.evaluate(&response);

    println!("\n📋 Response (first 500 chars):");
    println!("{}", truncate_safe(&response, 500));
    println!("\n📊 Quality score: {:.2}", quality.total);

    assert!(
        !response.is_empty(),
        "Model should return non-empty response"
    );
    if response.starts_with("ERROR:") {
        eprintln!(
            "⚠️  Skipping assertions — API error (likely rate limit): {}",
            truncate_safe(&response, 200)
        );
        return; // Don't fail the test suite on transient API errors
    }
    assert!(
        quality.total > 1.0,
        "Quality should be > 1.0 for a complex task, got {:.2}",
        quality.total
    );
}

#[tokio::test]
async fn test_openrouter_free_model() {
    let config = match load_config() {
        Some(c) => c,
        None => {
            eprintln!("SKIP: No config");
            return;
        }
    };

    let model_id = "z-ai/glm-4.5-air:free";
    let provider = build_openrouter_provider(model_id, &config.providers.openrouter.api_key);

    println!("\n🆓 FREE MODEL TEST: OpenRouter/{model_id}");

    let response = call_llm(
        &provider,
        model_id,
        "Implement binary search for a sorted array of integers. Include edge cases.",
    )
    .await;

    let detector = QualityDetector::new();
    let quality = detector.evaluate(&response);

    println!("  response len: {}", response.len());
    println!("  snippet: {}", truncate_safe(&response, 150));
    println!("  quality: total={:.2}", quality.total);

    assert!(
        !response.is_empty(),
        "Free model should return non-empty response"
    );
    if response.starts_with("ERROR:") {
        eprintln!(
            "⚠️  Free model API error: {}",
            truncate_safe(&response, 200)
        );
        return;
    }
    assert!(
        quality.total > 0.5,
        "Free model quality should be > 0.5, got {:.2}",
        quality.total
    );
}

#[tokio::test]
async fn test_ling_free_model() {
    let config = match load_config() {
        Some(c) => c,
        None => {
            eprintln!("SKIP: No config");
            return;
        }
    };

    let model_id = "inclusionai/ling-2.6-1t:free";
    let provider = build_openrouter_provider(model_id, &config.providers.openrouter.api_key);

    println!("\n🆓 LING MODEL TEST: OpenRouter/{model_id}");

    let response = call_llm(
        &provider,
        model_id,
        "Implement binary search for a sorted array of integers. Include edge cases.",
    )
    .await;

    let detector = QualityDetector::new();
    let quality = detector.evaluate(&response);

    println!("  response len: {}", response.len());
    println!("  snippet: {}", truncate_safe(&response, 150));
    println!("  quality: total={:.2}", quality.total);

    assert!(
        !response.is_empty(),
        "Ling model should return non-empty response"
    );
    if response.starts_with("ERROR:") {
        eprintln!(
            "⚠️  Ling model API error: {}",
            truncate_safe(&response, 200)
        );
        return;
    }
    assert!(
        quality.total > 0.5,
        "Ling model quality should be > 0.5, got {:.2}",
        quality.total
    );
}

async fn call_llm(provider: &Arc<dyn LLMProvider>, model: &str, prompt: &str) -> String {
    call_llm_with_retry(provider, model, prompt, 5).await
}

async fn call_llm_with_retry(
    provider: &Arc<dyn LLMProvider>,
    model: &str,
    prompt: &str,
    max_retries: u32,
) -> String {
    let request = CompletionRequest::new(
        model,
        vec![
            ChatMessage::system(
                "You are a senior software engineer. Provide detailed, well-structured responses.",
            ),
            ChatMessage::user(prompt.to_string()),
        ],
    )
    .with_max_tokens(2048)
    .with_temperature(0.7);

    for attempt in 0..=max_retries {
        match provider.complete(request.clone()).await {
            Ok(response) => return response.content,
            Err(e) => {
                let err_str = format!("{e}");
                if err_str.contains("Rate limit")
                    || err_str.contains("429")
                    || err_str.contains("rate")
                    || err_str.contains("Network error")
                {
                    if attempt < max_retries {
                        // Use server's Retry-After if available
                        let wait_duration = if let Some(delay) = e.retry_delay() {
                            delay
                        } else {
                            // Exponential backoff with jitter to avoid thundering herd
                            let base_secs = 30u64 * (attempt as u64 + 1);
                            // Use system time-based jitter (0-50% of base delay)
                            let now_nanos = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map_or(0, |d| d.as_nanos());
                            let jitter_ms = ((now_nanos as u64) % (base_secs * 500));
                            std::time::Duration::from_millis(base_secs * 1000 + jitter_ms)
                        };
                        let wait_secs = wait_duration.as_secs();
                        let wait_ms = wait_duration.subsec_millis();
                        eprintln!(
                            "    Rate limited, waiting {wait_secs}.{wait_ms:03}s (attempt {}/{max_retries})...",
                            attempt + 1
                        );
                        tokio::time::sleep(wait_duration).await;
                        continue;
                    }
                }
                eprintln!("    ERROR: {e}");
                return format!("ERROR: {e}");
            }
        }
    }
    "ERROR: max retries exceeded".to_string()
}

#[allow(dead_code)]
struct RunResult {
    model: String,
    task: String,
    complexity: f64,
    strategy: String,
    quality_before: f64,
    quality_after: f64,
    quality_delta: f64,
    response_len: usize,
    snippet: String,
    is_error: bool,
}

// ── A/B Test: Raw single-shot vs Pipeline (harness) ──────────────────

use rustycode_orchestration::config::OrchestrationConfig;
use rustycode_orchestration::pipeline::OrchestrationPipeline;
use rustycode_orchestration::task_context::TaskContext;

const AB_TASKS: &[(&str, &str)] = &[
    ("mips-interpreter", "Implement a MIPS interpreter supporting R-type, I-type, and J-type instructions with register file, memory, and branch handling"),
    ("binary-search", "Implement binary search for a sorted array of integers with proper edge case handling"),
    ("refactor-auth", "Refactor the authentication module to use JWT tokens instead of session cookies, update middleware, maintain backward compatibility"),
];

#[allow(clippy::too_many_lines)]
#[tokio::test]
async fn test_ab_raw_vs_pipeline() {
    let config = match load_config() {
        Some(c) => c,
        None => {
            eprintln!("SKIP: No config");
            return;
        }
    };

    let model_id = "glm-5.1";
    let base_url = config
        .providers
        .openai
        .base_url
        .as_deref()
        .unwrap_or("https://api.z.ai/api/coding/paas/v4");
    let provider = build_zai_provider(model_id, &config.providers.openai.api_key, base_url);
    let detector = QualityDetector::new();

    println!("\n🔬 A/B TEST: Raw single-shot vs Pipeline (harness)");
    println!("Model: {model_id} via z.ai direct");
    println!("{}", "=".repeat(60));

    let mut ab_results: Vec<(String, f64, f64)> = Vec::new();

    for (task_idx, &(task_id, task_desc)) in AB_TASKS.iter().enumerate() {
        if task_idx > 0 {
            eprintln!("  ⏳ Cooldown 15s between tasks...");
            tokio::time::sleep(std::time::Duration::from_secs(15)).await;
        }
        println!("\n── Task: {task_id} ──");

        // B: Pipeline FIRST (before raw, to avoid rate limit burning)
        let orch_config = OrchestrationConfig::default();
        let pipeline =
            OrchestrationPipeline::with_provider_and_model(orch_config, provider.clone(), model_id);
        let mut ctx = TaskContext::new(format!("ab-{task_id}"), task_desc.to_string());

        let pipeline_output = match pipeline.orchestrator().think_deep(&mut ctx).await {
            Ok(output) => {
                if output.len() < 200 {
                    eprintln!(
                        "  [PIPELINE] short output ({len} chars): {output}",
                        len = output.len()
                    );
                    if let Some(graph) = &ctx.reasoning_graph {
                        eprintln!("  [PIPELINE] reasoning graph has {} thoughts", graph.len());
                        for (i, t) in graph.thoughts().enumerate() {
                            eprintln!(
                                "    [{i}] kind={:?} conf={:.2} content={}",
                                t.kind,
                                t.metadata.confidence,
                                truncate_safe(&t.content, 80)
                            );
                        }
                    }
                }
                output
            }
            Err(e) => {
                eprintln!("  [PIPELINE] error: {e}");
                format!("ERROR: {e}")
            }
        };
        let pipe_quality = detector.evaluate(&pipeline_output).total;
        let pipe_is_error = pipeline_output.starts_with("ERROR:");
        println!(
            "  [PIPELINE] quality={pipe_quality:.2} len={} {}",
            pipeline_output.len(),
            if pipe_is_error { "❌" } else { "✅" }
        );

        // A: Raw single-shot
        let raw_response = call_llm(&provider, model_id, task_desc).await;
        let raw_quality = detector.evaluate(&raw_response).total;
        let raw_is_error = raw_response.starts_with("ERROR:");
        println!(
            "  [RAW]      quality={raw_quality:.2} len={} {}",
            raw_response.len(),
            if raw_is_error { "❌" } else { "✅" }
        );

        let delta = pipe_quality - raw_quality;
        let verdict = if pipe_is_error && !raw_is_error {
            "PIPELINE ERROR (raw ok)"
        } else if delta > 0.3 {
            "🏆 PIPELINE WINS"
        } else if delta < -0.3 {
            "🏆 RAW WINS"
        } else {
            "≈ TIE"
        };
        println!("  → Δ{delta:+.2} {verdict}");

        ab_results.push((task_id.to_string(), raw_quality, pipe_quality));
    }

    // Summary
    println!("\n📈 A/B SUMMARY");
    println!("{}", "=".repeat(60));
    let raw_avg = ab_results.iter().map(|(_, r, _)| *r).sum::<f64>() / ab_results.len() as f64;
    let pipe_avg = ab_results.iter().map(|(_, _, p)| *p).sum::<f64>() / ab_results.len() as f64;
    println!("  Raw avg quality:      {raw_avg:.2}");
    println!("  Pipeline avg quality: {pipe_avg:.2}");
    println!("  Delta:                {:+.2}", pipe_avg - raw_avg);

    let pipe_wins = ab_results.iter().filter(|(_, r, p)| *p > *r + 0.1).count();
    let raw_wins = ab_results.iter().filter(|(_, r, p)| *r > *p + 0.1).count();
    println!(
        "  Pipeline wins: {pipe_wins}/{ab_len}",
        ab_len = ab_results.len()
    );
    println!(
        "  Raw wins:      {raw_wins}/{ab_len}",
        ab_len = ab_results.len()
    );
}

// ── A/B Test: Ling 2.6-1T via OpenRouter (free) ─────────────────────

#[allow(clippy::too_many_lines)]
#[tokio::test]
async fn test_ab_ling_free() {
    let config = match load_config() {
        Some(c) => c,
        None => {
            eprintln!("SKIP: No config");
            return;
        }
    };

    let model_id = "inclusionai/ling-2.6-1t:free";
    let provider = build_openrouter_provider(model_id, &config.providers.openrouter.api_key);
    let detector = QualityDetector::new();

    println!("\n🔬 A/B TEST: Raw single-shot vs Pipeline — Ling 2.6-1T (free/OpenRouter)");
    println!("{}", "=".repeat(60));

    let mut ab_results: Vec<(String, f64, f64)> = Vec::new();

    for (task_idx, &(task_id, task_desc)) in AB_TASKS.iter().enumerate() {
        if task_idx > 0 {
            eprintln!("  ⏳ Cooldown 30s between tasks (free tier)...");
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        }
        println!("\n── Task: {task_id} ──");

        // B: Pipeline FIRST
        let orch_config = OrchestrationConfig::default();
        let pipeline =
            OrchestrationPipeline::with_provider_and_model(orch_config, provider.clone(), model_id);
        let mut ctx = TaskContext::new(format!("ab-{task_id}"), task_desc.to_string());

        let pipeline_output = match pipeline.orchestrator().think_deep(&mut ctx).await {
            Ok(output) => {
                if output.len() < 200 {
                    eprintln!(
                        "  [PIPELINE] short output ({len} chars): {output}",
                        len = output.len()
                    );
                    if let Some(graph) = &ctx.reasoning_graph {
                        eprintln!("  [PIPELINE] reasoning graph has {} thoughts", graph.len());
                        for (i, t) in graph.thoughts().enumerate() {
                            eprintln!(
                                "    [{i}] kind={:?} conf={:.2} content={}",
                                t.kind,
                                t.metadata.confidence,
                                truncate_safe(&t.content, 80)
                            );
                        }
                    }
                }
                output
            }
            Err(e) => {
                eprintln!("  [PIPELINE] error: {e}");
                format!("ERROR: {e}")
            }
        };
        let pipe_quality = detector.evaluate(&pipeline_output).total;
        let pipe_is_error = pipeline_output.starts_with("ERROR:");
        println!(
            "  [PIPELINE] quality={pipe_quality:.2} len={} {}",
            pipeline_output.len(),
            if pipe_is_error { "❌" } else { "✅" }
        );

        // Cooldown between pipeline and raw
        eprintln!("  ⏳ Cooldown 30s before raw call...");
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;

        // A: Raw single-shot
        let raw_response = call_llm(&provider, model_id, task_desc).await;
        let raw_quality = detector.evaluate(&raw_response).total;
        let raw_is_error = raw_response.starts_with("ERROR:");
        println!(
            "  [RAW]      quality={raw_quality:.2} len={} {}",
            raw_response.len(),
            if raw_is_error { "❌" } else { "✅" }
        );

        let delta = pipe_quality - raw_quality;
        let verdict = if pipe_is_error && !raw_is_error {
            "PIPELINE ERROR (raw ok)"
        } else if delta > 0.3 {
            "🏆 PIPELINE WINS"
        } else if delta < -0.3 {
            "🏆 RAW WINS"
        } else {
            "≈ TIE"
        };
        println!("  → Δ{delta:+.2} {verdict}");

        ab_results.push((task_id.to_string(), raw_quality, pipe_quality));
    }

    // Summary
    println!("\n📈 A/B SUMMARY — Ling 2.6-1T (free)");
    println!("{}", "=".repeat(60));
    let raw_avg = ab_results.iter().map(|(_, r, _)| *r).sum::<f64>() / ab_results.len() as f64;
    let pipe_avg = ab_results.iter().map(|(_, _, p)| *p).sum::<f64>() / ab_results.len() as f64;
    println!("  Raw avg quality:      {raw_avg:.2}");
    println!("  Pipeline avg quality: {pipe_avg:.2}");
    println!("  Delta:                {:+.2}", pipe_avg - raw_avg);

    let pipe_wins = ab_results.iter().filter(|(_, r, p)| *p > *r + 0.1).count();
    let raw_wins = ab_results.iter().filter(|(_, r, p)| *r > *p + 0.1).count();
    println!(
        "  Pipeline wins: {pipe_wins}/{ab_len}",
        ab_len = ab_results.len()
    );
    println!(
        "  Raw wins:      {raw_wins}/{ab_len}",
        ab_len = ab_results.len()
    );
}
