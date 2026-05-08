//! SWE-bench evaluation command
//!
//! Runs RustyCode on SWE-bench instances and produces evaluation-ready
//! predictions in the standard format. Honest evaluation — no tricks.

use anyhow::Result;
use std::path::PathBuf;

use rustycode_bench::swebench::{SweBenchConfig, run_swebench as run_swebench_eval};
use rustycode_bench::CodeAgentConfig;

/// Arguments for the `rustycode swebench` subcommand
#[derive(Debug, clap::Args)]
pub struct SweBenchArgs {
    /// Path to SWE-bench instances JSON file
    #[arg(long)]
    pub instances: PathBuf,

    /// Output path for predictions
    #[arg(long, default_value = "predictions.json")]
    pub output: PathBuf,

    /// Model to use (e.g. claude-sonnet-4-6)
    #[arg(long, default_value = "claude-sonnet-4-6")]
    pub model: String,

    /// LLM provider: anthropic, openai
    #[arg(long, default_value = "anthropic")]
    pub provider: String,

    /// Max tool-use turns per instance
    #[arg(long, default_value_t = 30)]
    pub max_turns: usize,

    /// Max tokens per LLM response
    #[arg(long, default_value_t = 16_384)]
    pub max_tokens: u32,

    /// Timeout per instance in seconds
    #[arg(long, default_value_t = 600)]
    pub timeout: u64,

    /// Number of instances to run in parallel (currently 1)
    #[arg(long, default_value_t = 1)]
    pub parallel: usize,

    /// Specific instance IDs to run (comma-separated)
    #[arg(long)]
    pub instance_ids: Option<String>,

    /// Output format: json (array) or jsonl (one per line)
    #[arg(long, default_value = "json")]
    pub format: String,

    /// Working directory for cloned repos
    #[arg(long, default_value = "swebench-work")]
    pub work_dir: PathBuf,
}

/// Execute the SWE-bench evaluation command
pub async fn run_swebench(args: SweBenchArgs) -> Result<()> {
    println!("SWE-bench Evaluation");
    println!("  Instances: {}", args.instances.display());
    println!("  Output:    {}", args.output.display());
    println!("  Model:     {} ({})", args.model, args.provider);
    println!("  Turns:     {} max", args.max_turns);
    println!("  Timeout:   {}s/instance", args.timeout);
    println!();

    // Parse comma-separated instance IDs if provided
    let instance_ids = args.instance_ids.as_ref().map(|ids| {
        ids.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
    });

    if let Some(ref ids) = instance_ids {
        if !ids.is_empty() {
            println!("  Selected:  {} instance(s)", ids.len());
        }
    }

    let config = SweBenchConfig {
        instances_path: args.instances,
        output_path: args.output,
        format: args.format,
        model_name: args.model.clone(),
        instance_ids,
        work_dir: args.work_dir,
        agent_config: CodeAgentConfig {
            model: args.model,
            provider: args.provider,
            max_turns: args.max_turns,
            max_tokens: args.max_tokens,
            ..Default::default()
        },
        timeout_secs: args.timeout,
    };

    run_swebench_eval(config).await?;

    Ok(())
}
