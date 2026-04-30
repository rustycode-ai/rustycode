//! SWE-bench evaluation command
//!
//! Runs RustyCode on SWE-bench instances and produces evaluation-ready
//! predictions in the standard format.

use anyhow::Result;
use clap::Args;
use std::path::PathBuf;

use rustycode_orchestration::swebench::SweBenchRunner;

/// Arguments for the `rustycode swebench` subcommand
#[derive(Debug, Args)]
pub struct SweBenchArgs {
    /// Path to SWE-bench instances JSON file
    #[arg(long)]
    pub instances: PathBuf,

    /// Output path for predictions
    #[arg(long, default_value = "predictions.json")]
    pub output: PathBuf,

    /// Cost budget per instance (dollars)
    #[arg(long, default_value = "0.50")]
    pub budget: f64,

    /// Number of instances to run in parallel
    #[arg(long, default_value = "1")]
    pub parallel: usize,

    /// Specific instance IDs to run (comma-separated)
    #[arg(long)]
    pub instance_ids: Option<String>,

    /// Output format: json (array) or jsonl (one per line)
    #[arg(long, default_value = "json")]
    pub format: String,
}

/// Execute the SWE-bench evaluation command
pub async fn run_swebench(args: SweBenchArgs) -> Result<()> {
    println!("SWE-bench Evaluation Runner");
    println!("  Instances: {}", args.instances.display());
    println!("  Output:    {}", args.output.display());
    println!("  Budget:    ${:.2}/instance", args.budget);
    println!("  Parallel:  {}", args.parallel);
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
            println!("  IDs:       {} instance(s) selected", ids.len());
        }
    }

    let mut runner = SweBenchRunner::new(
        args.instances,
        args.output,
        args.budget,
        args.parallel,
        instance_ids,
    );
    runner.format = args.format;

    let predictions = runner.run_all().await?;

    println!();
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("SWE-bench evaluation complete");
    println!("  Total:   {} instance(s)", predictions.len());
    let succeeded = predictions
        .iter()
        .filter(|p| !p.model_patch.is_empty())
        .count();
    println!(
        "  Results: {} with patches, {} empty",
        succeeded,
        predictions.len() - succeeded
    );
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    Ok(())
}
