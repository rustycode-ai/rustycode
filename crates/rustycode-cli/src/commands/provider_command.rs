//! Provider and model management commands
//!
//! Allows users to:
//! - List available providers and their models
//! - Show detailed information about models
//! - Configure task-specific model selection
//! - Estimate costs for different model tiers

use clap::Subcommand;
use rustycode_litert::{ensure_litert_lm_runtime, LiteRtLmInstallConfig};
use rustycode_llm::{provider_helpers, ModelTier};

#[derive(Debug, Subcommand)]
#[non_exhaustive]
pub enum ProviderCommand {
    /// List all available providers
    List,
    /// Show models from a specific provider
    Models {
        /// Provider ID (e.g., anthropic, openai, gemini)
        #[arg(value_name = "PROVIDER")]
        provider: Option<String>,
    },
    /// Show detailed information about a model
    Info {
        /// Model ID (e.g., claude-3-opus, gpt-4)
        model: String,
    },
    /// List models by cost tier
    Tiers,
    /// Estimate execution cost for different model tiers
    Cost {
        /// Number of input tokens
        #[arg(long, default_value = "10000")]
        input_tokens: usize,
        /// Number of output tokens
        #[arg(long, default_value = "5000")]
        output_tokens: usize,
    },
    /// Show the unified model catalog from rustycode-providers
    Catalog,
    /// Install the LiteRT-LM runtime and default model
    Install {
        /// Provider to install
        #[arg(value_name = "PROVIDER")]
        provider: String,
    },
}

pub async fn execute(command: ProviderCommand, format: &str) -> anyhow::Result<()> {
    match command {
        ProviderCommand::List => cmd_list_providers(format),
        ProviderCommand::Models { provider } => cmd_list_models(provider, format),
        ProviderCommand::Info { model } => cmd_model_info(&model),
        ProviderCommand::Tiers => cmd_show_tiers(),
        ProviderCommand::Cost {
            input_tokens,
            output_tokens,
        } => cmd_estimate_cost(input_tokens, output_tokens),
        ProviderCommand::Catalog => cmd_show_catalog().await,
        ProviderCommand::Install { provider } => cmd_install_provider(&provider).await,
    }
}

fn cmd_list_providers(format: &str) -> anyhow::Result<()> {
    let providers = provider_helpers::list_providers();

    if format == "json" {
        let registry = provider_helpers::get_registry();
        let provider_list: Vec<serde_json::Value> = providers
            .iter()
            .filter_map(|pid| {
                registry.get_provider(pid).map(|p| {
                    serde_json::json!({
                        "id": p.id,
                        "name": p.name,
                        "description": p.description,
                    })
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&provider_list)?);
    } else {
        println!("\n📦 Available Providers:\n");
        println!("{:<15} {:<40}", "Provider ID", "Description");
        println!("{}", "-".repeat(55));

        for provider_id in providers {
            let registry = provider_helpers::get_registry();
            if let Some(provider) = registry.get_provider(&provider_id) {
                println!("{:<15} {:<40}", provider.id, provider.description);
            }
        }

        println!();
    }
    Ok(())
}

fn cmd_list_models(provider: Option<String>, format: &str) -> anyhow::Result<()> {
    if let Some(provider_id) = provider {
        // Show models for specific provider
        let registry = provider_helpers::get_registry();
        if let Some(provider_meta) = registry.get_provider(&provider_id) {
            if format == "json" {
                let models_json: Vec<serde_json::Value> = provider_meta
                    .models
                    .iter()
                    .map(|m| {
                        serde_json::json!({
                            "id": m.id,
                            "tier": format!("{:?}", m.tier),
                            "context_window": m.context_window,
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&models_json)?);
            } else {
                println!("\n📦 {} Models:\n", provider_meta.name);
                println!("{:<30} {:<15} {:<12}", "Model ID", "Tier", "Context Window");
                println!("{}", "-".repeat(60));

                for model in &provider_meta.models {
                    let tier = format!("{:?}", model.tier);
                    println!(
                        "{:<30} {:<15} {:<12}",
                        model.id,
                        tier,
                        format!("{}k", model.context_window / 1000)
                    );
                }
                println!();
            }
        } else {
            eprintln!("Provider '{}' not found", provider_id);
            return Ok(());
        }
    } else {
        // Show all models
        let models = provider_helpers::list_models();
        if format == "json" {
            let mut models_json = Vec::new();
            for model_id in &models {
                if let Some((_, pid)) = provider_helpers::find_model_provider(model_id) {
                    let registry = provider_helpers::get_registry();
                    if let Some(model_info) = registry
                        .get_provider(&pid)
                        .and_then(|p| p.models.iter().find(|m| m.id == *model_id))
                    {
                        models_json.push(serde_json::json!({
                            "id": model_id,
                            "provider": pid,
                            "tier": format!("{:?}", model_info.tier),
                            "context_window": model_info.context_window,
                        }));
                    }
                }
            }
            println!("{}", serde_json::to_string_pretty(&models_json)?);
        } else {
            println!("\n📦 All Available Models:\n");
            println!("{:<30} {:<15} {:<15}", "Model ID", "Provider", "Tier");
            println!("{}", "-".repeat(60));

            for model_id in models {
                if let Some((_, provider_id)) = provider_helpers::find_model_provider(&model_id) {
                    let registry = provider_helpers::get_registry();
                    if let Some(model_info) = registry
                        .get_provider(&provider_id)
                        .and_then(|p| p.models.iter().find(|m| m.id == model_id))
                    {
                        let tier = format!("{:?}", model_info.tier);
                        println!("{:<30} {:<15} {:<15}", model_id, provider_id, tier);
                    }
                }
            }
            println!();
        }
    }

    Ok(())
}

fn cmd_model_info(model: &str) -> anyhow::Result<()> {
    if let Some((_, provider_id)) = provider_helpers::find_model_provider(model) {
        let registry = provider_helpers::get_registry();
        if let Some(provider) = registry.get_provider(&provider_id) {
            if let Some(model_info) = provider.models.iter().find(|m| m.id == model) {
                println!("\n📊 Model Information: {}\n", model);
                println!("Provider:        {}", provider.name);
                println!("Tier:            {:?}", model_info.tier);
                println!("Context Window:  {} tokens", model_info.context_window);
                println!("Vision Support:  {}", model_info.supports_vision);
                println!("Tools Support:   {}", model_info.supports_tools);
                println!("Release Date:    {}", model_info.release_date);
                println!(
                    "Cost (input):    ${:.6}/1M tokens",
                    model_info.cost_per_1m_input
                );
                println!(
                    "Cost (output):   ${:.6}/1M tokens",
                    model_info.cost_per_1m_output
                );
                println!();
                return Ok(());
            }
        }
    }
    eprintln!("Model '{}' not found", model);
    Ok(())
}

fn cmd_show_tiers() -> anyhow::Result<()> {
    let registry = provider_helpers::get_registry();

    println!("\n💰 Models by Cost Tier:\n");

    // Budget tier
    let budget_models = registry.get_models_by_tier(ModelTier::Budget);
    println!("🟢 Budget Tier (Cheapest):");
    for model in budget_models {
        println!(
            "  - {} (${:.6}/1M input)",
            model.id, model.cost_per_1m_input
        );
    }

    // Balanced tier
    let balanced_models = registry.get_models_by_tier(ModelTier::Balanced);
    println!("\n🟡 Balanced Tier (Good value):");
    for model in balanced_models {
        println!(
            "  - {} (${:.6}/1M input)",
            model.id, model.cost_per_1m_input
        );
    }

    // Premium tier
    let premium_models = registry.get_models_by_tier(ModelTier::Premium);
    println!("\n🔴 Premium Tier (Most capable):");
    for model in premium_models {
        println!(
            "  - {} (${:.6}/1M input)",
            model.id, model.cost_per_1m_input
        );
    }

    println!();
    Ok(())
}

fn cmd_estimate_cost(input_tokens: usize, output_tokens: usize) -> anyhow::Result<()> {
    let registry = provider_helpers::get_registry();

    println!(
        "\n💸 Cost Estimation (input: {} tokens, output: {} tokens):\n",
        input_tokens, output_tokens
    );

    println!("{:<30} {:<15} {:<15}", "Model", "Tier", "Estimated Cost");
    println!("{}", "-".repeat(60));

    // Budget tier
    let budget_models = registry.get_models_by_tier(ModelTier::Budget);
    if !budget_models.is_empty() {
        let first = &budget_models[0];
        let cost = (first.cost_per_1m_input * input_tokens as f64 / 1_000_000_f64)
            + (first.cost_per_1m_output * output_tokens as f64 / 1_000_000_f64);
        println!("{:<30} {:<15} ${:.4}", first.id, "Budget", cost);
    }

    // Balanced tier
    let balanced_models = registry.get_models_by_tier(ModelTier::Balanced);
    if !balanced_models.is_empty() {
        let first = &balanced_models[0];
        let cost = (first.cost_per_1m_input * input_tokens as f64 / 1_000_000_f64)
            + (first.cost_per_1m_output * output_tokens as f64 / 1_000_000_f64);
        println!("{:<30} {:<15} ${:.4}", first.id, "Balanced", cost);
    }

    // Premium tier
    let premium_models = registry.get_models_by_tier(ModelTier::Premium);
    if !premium_models.is_empty() {
        let first = &premium_models[0];
        let cost = (first.cost_per_1m_input * input_tokens as f64 / 1_000_000_f64)
            + (first.cost_per_1m_output * output_tokens as f64 / 1_000_000_f64);
        println!("{:<30} {:<15} ${:.4}", first.id, "Premium", cost);
    }

    // Calculate savings
    if let Some(premium) = premium_models.first() {
        let premium_cost = (premium.cost_per_1m_input * input_tokens as f64 / 1_000_000_f64)
            + (premium.cost_per_1m_output * output_tokens as f64 / 1_000_000_f64);

        if let Some(budget) = budget_models.first() {
            let budget_cost = (budget.cost_per_1m_input * input_tokens as f64 / 1_000_000_f64)
                + (budget.cost_per_1m_output * output_tokens as f64 / 1_000_000_f64);

            let savings_percent = ((premium_cost - budget_cost) / premium_cost * 100.0) as i32;
            println!(
                "\n💡 Using budget tier saves ~{}% vs premium",
                savings_percent
            );
        }
    }

    println!();
    Ok(())
}

async fn cmd_show_catalog() -> anyhow::Result<()> {
    use rustycode_providers::{bootstrap_from_env, predefined};

    let registry = {
        let r = bootstrap_from_env().await;

        // Ensure all predefined models are registered even if no API key is set
        // (so we can see what's available in the catalog)
        for m in predefined::anthropic_models() {
            r.register_model("anthropic", m).await;
        }
        for m in predefined::openai_models() {
            r.register_model("openai", m).await;
        }
        for m in predefined::openrouter_models() {
            r.register_model("openrouter", m).await;
        }
        for m in predefined::gemini_models() {
            r.register_model("gemini", m).await;
        }
        for m in predefined::groq_models() {
            r.register_model("groq", m).await;
        }
        for m in predefined::copilot_models() {
            r.register_model("copilot", m).await;
        }
        for m in predefined::zhipu_models() {
            r.register_model("zhipu", m).await;
        }
        for m in predefined::ollama_models() {
            r.register_model("ollama", m).await;
        }
        for m in predefined::vertex_models() {
            r.register_model("vertex", m).await;
        }
        for m in predefined::kimi_cn_models() {
            r.register_model("kimi-cn", m).await;
        }
        for m in predefined::kimi_global_models() {
            r.register_model("kimi-global", m).await;
        }
        for m in predefined::alibaba_cn_models() {
            r.register_model("alibaba-cn", m).await;
        }
        for m in predefined::alibaba_global_models() {
            r.register_model("alibaba-global", m).await;
        }

        r
    };

    let models = registry.list_all_models().await;

    println!("\n📚 Unified Model Catalog:\n");
    println!(
        "{:<35} {:<15} {:<15} {:<15}",
        "Model ID", "Provider", "Context", "Cost (In/Out 1M)"
    );
    println!("{}", "-".repeat(85));

    let mut sorted_models = models;
    sorted_models.sort_by(|a, b| {
        let p_cmp = a.provider_id.cmp(&b.provider_id);
        if p_cmp == std::cmp::Ordering::Equal {
            a.id.cmp(&b.id)
        } else {
            p_cmp
        }
    });

    for model in sorted_models {
        let context = if model.context_window >= 1_000_000 {
            format!("{:.1}M", model.context_window as f64 / 1_000_000.0)
        } else {
            format!("{}k", model.context_window / 1000)
        };

        let pricing = if model.is_free() {
            "Free".to_string()
        } else {
            format!(
                "${:.2} / ${:.2}",
                model.input_cost_per_1k * 1000.0,
                model.output_cost_per_1k * 1000.0
            )
        };

        println!(
            "{:<35} {:<15} {:<15} {:<15}",
            model.id, model.provider_id, context, pricing
        );
    }

    println!("\n💡 Pricing is shown as Cost per 1 Million tokens (Input / Output).");
    println!("   Use 'rustycode provider info <MODEL>' for more details.\n");

    Ok(())
}

async fn cmd_install_provider(provider: &str) -> anyhow::Result<()> {
    match provider {
        "litert-lm" | "litertlm" | "litert_lm" => {
            let config = LiteRtLmInstallConfig::default();
            let result = ensure_litert_lm_runtime(&config).await?;
            println!("\n📦 LiteRT-LM installed successfully:\n");
            println!("Install dir: {}", result.install_dir.display());
            println!("Binary:      {}", result.binary_path.display());
            println!("Model:       {}", result.model_path.display());
            println!();
            Ok(())
        }
        other => {
            eprintln!(
                "Provider '{}' does not support automatic installation yet",
                other
            );
            Ok(())
        }
    }
}
