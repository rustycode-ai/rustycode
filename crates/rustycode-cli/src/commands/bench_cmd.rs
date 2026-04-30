//! `rustycode bench` subcommand — benchmark runner for agent evaluation.

use anyhow::Result;
use std::path::PathBuf;

use super::cli_args::BenchCommand;

pub async fn execute(cmd: BenchCommand) -> Result<()> {
    match cmd {
        BenchCommand::Run {
            dataset,
            path,
            agent,
            model,
            provider,
            n_concurrent,
            force_build,
            cleanup,
            job_name,
            jobs_dir,
            max_turns,
            max_tokens,
            timeout,
        } => {
            run_bench(
                dataset,
                path,
                agent,
                model,
                provider,
                n_concurrent,
                force_build,
                cleanup,
                job_name,
                jobs_dir,
                max_turns,
                max_tokens,
                timeout,
            )
            .await
        }
        BenchCommand::Results { job_dir } => show_results(job_dir),
        BenchCommand::ListDatasets => list_datasets(),
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_bench(
    dataset: Option<String>,
    path: Option<PathBuf>,
    agent: String,
    model: String,
    provider: String,
    n_concurrent: usize,
    force_build: bool,
    cleanup: bool,
    job_name: Option<String>,
    jobs_dir: Option<PathBuf>,
    max_turns: usize,
    max_tokens: u32,
    timeout: u64,
) -> Result<()> {
    // Resolve dataset directory
    let dataset_dir = if let Some(p) = path {
        p
    } else if let Some(ref ds) = dataset {
        let registry = rustycode_bench::DatasetRegistry::new();
        registry.resolve(ds)?
    } else {
        anyhow::bail!("Specify --path or --dataset");
    };

    // Job config
    let job_name = job_name.unwrap_or_else(|| {
        let ts = chrono::Local::now().format("%Y%m%d-%H%M%S");
        format!("bench-{ts}")
    });
    let jobs_dir = jobs_dir.unwrap_or_else(|| PathBuf::from("jobs"));

    let job_config = rustycode_bench::JobConfig {
        job_name: job_name.clone(),
        jobs_dir,
        n_concurrent,
        force_build,
        cleanup,
    };

    println!("Dataset: {}", dataset_dir.display());
    println!("Job: {job_name}");
    println!("Agent: {agent} (model: {model}, provider: {provider})");
    println!("Concurrency: {n_concurrent}");

    async_run(
        &dataset_dir,
        agent,
        model,
        provider,
        job_config,
        max_turns,
        max_tokens,
        timeout,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn async_run(
    dataset_dir: &std::path::Path,
    agent_name: String,
    model: String,
    provider: String,
    job_config: rustycode_bench::JobConfig,
    max_turns: usize,
    max_tokens: u32,
    timeout: u64,
) -> Result<()> {
    let job = rustycode_bench::Job::new(job_config);

    match agent_name.as_str() {
        "oracle" => {
            let agent_factory = |solution_dir: PathBuf| -> Box<dyn rustycode_bench::BenchAgent> {
                Box::new(rustycode_bench::OracleAgent::new(solution_dir))
            };
            let verifier_factory =
                |tests_dir: PathBuf, timeout_secs: u64| -> Box<dyn rustycode_bench::Verifier> {
                    Box::new(rustycode_bench::ScriptVerifier::new(
                        tests_dir,
                        timeout_secs,
                    ))
                };

            let results = job
                .run(dataset_dir, &agent_factory, &verifier_factory)
                .await?;
            println!("\n{}", results.summary());
        }
        "nop" => {
            let agent_factory = |_solution_dir: PathBuf| -> Box<dyn rustycode_bench::BenchAgent> {
                Box::new(rustycode_bench::NopAgent)
            };
            let verifier_factory =
                |tests_dir: PathBuf, timeout_secs: u64| -> Box<dyn rustycode_bench::Verifier> {
                    Box::new(rustycode_bench::ScriptVerifier::new(
                        tests_dir,
                        timeout_secs,
                    ))
                };

            let results = job
                .run(dataset_dir, &agent_factory, &verifier_factory)
                .await?;
            println!("\n{}", results.summary());
        }
        "code" => {
            let agent_factory = {
                let model = model.clone();
                let provider = provider.clone();
                move |solution_dir: PathBuf| -> Box<dyn rustycode_bench::BenchAgent> {
                    let config = rustycode_bench::CodeAgentConfig {
                        model: model.clone(),
                        provider: provider.clone(),
                        max_turns,
                        max_tokens,
                        command_timeout_secs: timeout,
                        ..Default::default()
                    };
                    match rustycode_bench::CodeAgent::auto(config) {
                        Ok(agent) => {
                            let _ = solution_dir; // Code agent doesn't use solution_dir
                            Box::new(agent) as Box<dyn rustycode_bench::BenchAgent>
                        }
                        Err(e) => {
                            tracing::error!("Failed to create code agent: {e}");
                            Box::new(rustycode_bench::NopAgent)
                                as Box<dyn rustycode_bench::BenchAgent>
                        }
                    }
                }
            };
            let verifier_factory =
                |tests_dir: PathBuf, timeout_secs: u64| -> Box<dyn rustycode_bench::Verifier> {
                    Box::new(rustycode_bench::ScriptVerifier::new(
                        tests_dir,
                        timeout_secs,
                    ))
                };

            let results = job
                .run(dataset_dir, &agent_factory, &verifier_factory)
                .await?;
            println!("\n{}", results.summary());
        }
        other => {
            anyhow::bail!("Unknown agent: '{other}'. Supported: oracle, nop, code");
        }
    }

    Ok(())
}

fn show_results(job_dir: PathBuf) -> Result<()> {
    let result_path = job_dir.join("result.json");
    if !result_path.exists() {
        anyhow::bail!("No results found at {}", result_path.display());
    }

    let content = std::fs::read_to_string(&result_path)?;
    let results: rustycode_bench::BenchmarkResults = serde_json::from_str(&content)?;
    println!("{}", results.summary());
    Ok(())
}

fn list_datasets() -> Result<()> {
    let registry = rustycode_bench::DatasetRegistry::new();
    let datasets = registry.list_datasets();

    if datasets.is_empty() {
        println!("No datasets found.");
        println!("Searched: ~/.cache/harbor/tasks/");
        return Ok(());
    }

    println!("Available datasets:\n");
    for ds in &datasets {
        println!(
            "  {} ({} tasks) — {}",
            ds.name,
            ds.task_count,
            ds.path.display()
        );
    }
    Ok(())
}
