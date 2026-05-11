//! `rustycode bench` subcommand — benchmark runner for agent evaluation.

use anyhow::Result;
use std::path::{Path, PathBuf};

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
            env,
            output,
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
                env,
                output,
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
    env: String,
    _output: Option<String>,
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

    println!("Dataset: {}", dataset_dir.display());
    println!("Job: {job_name}");
    println!("Agent: {agent} (model: {model}, provider: {provider})");
    println!("Concurrency: {n_concurrent}");
    println!("Environment: {env}");

    let tasks = rustycode_bench::ResolvedTask::discover(&dataset_dir)?;
    println!("Tasks: {}", tasks.len());

    match env.as_str() {
        "native" => {
            run_native(
                &tasks,
                &dataset_dir,
                &agent,
                &model,
                &provider,
                n_concurrent,
                &job_name,
                max_turns,
                max_tokens,
                timeout,
                _output.as_deref(),
            )
            .await
        }
        "docker" => {
            let jobs_dir = jobs_dir.unwrap_or_else(|| PathBuf::from("jobs"));
            let job_config = rustycode_bench::JobConfig {
                job_name: job_name.clone(),
                jobs_dir,
                n_concurrent,
                force_build,
                cleanup,
            };
            async_run(
                &tasks, agent, model, provider, job_config, max_turns, max_tokens, timeout,
            )
            .await
        }
        other => anyhow::bail!("Unknown environment: '{other}'. Use 'native' or 'docker'"),
    }
}

#[allow(clippy::too_many_arguments)]
async fn async_run(
    tasks: &[rustycode_bench::ResolvedTask],
    agent_name: String,
    model: String,
    provider: String,
    job_config: rustycode_bench::JobConfig,
    max_turns: usize,
    max_tokens: u32,
    _timeout: u64,
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

            let results = job.run(tasks, &agent_factory, &verifier_factory).await?;
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

            let results = job.run(tasks, &agent_factory, &verifier_factory).await?;
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

            let results = job.run(tasks, &agent_factory, &verifier_factory).await?;
            println!("\n{}", results.summary());
        }
        other => {
            anyhow::bail!("Unknown agent: '{other}'. Supported: oracle, nop, code");
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn run_native(
    tasks: &[rustycode_bench::ResolvedTask],
    dataset_path: &Path,
    agent_name: &str,
    model: &str,
    provider: &str,
    n_concurrent: usize,
    job_name: &str,
    max_turns: usize,
    max_tokens: u32,
    timeout: u64,
) -> Result<()> {
    let runner_config = rustycode_bench::NativeRunnerConfig {
        agent_name: agent_name.to_string(),
        model: model.to_string(),
        n_concurrent,
        job_name: job_name.to_string(),
        retry_config: rustycode_bench::RetryConfig::default(),
        per_trial_timeout: timeout,
    };
    let runner = rustycode_bench::NativeRunner::new(runner_config);

    let provider_owned = provider.to_string();
    let agent_factory: rustycode_bench::AgentFactory = Box::new(
        move |name: &str, mdl: &str, solution_dir: PathBuf| match name {
            "oracle" => Ok(Box::new(rustycode_bench::OracleAgent::new(solution_dir))
                as Box<dyn rustycode_bench::BenchAgent>),
            "nop" => {
                let _ = solution_dir;
                Ok(Box::new(rustycode_bench::NopAgent) as Box<dyn rustycode_bench::BenchAgent>)
            }
            "code" => {
                let config = rustycode_bench::CodeAgentConfig {
                    model: mdl.to_string(),
                    provider: provider_owned.clone(),
                    max_turns,
                    max_tokens,
                    ..Default::default()
                };
                match rustycode_bench::CodeAgent::auto(config) {
                    Ok(agent) => {
                        let _ = solution_dir;
                        Ok(Box::new(agent) as Box<dyn rustycode_bench::BenchAgent>)
                    }
                    Err(e) => {
                        tracing::error!("Failed to create code agent: {e}");
                        Ok(Box::new(rustycode_bench::NopAgent)
                            as Box<dyn rustycode_bench::BenchAgent>)
                    }
                }
            }
            other => anyhow::bail!("Unknown agent: '{other}'. Supported: oracle, nop, code"),
        },
    );

    let results = runner.run(tasks, dataset_path, agent_factory).await?;
    println!("\n{}", results.summary());
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
