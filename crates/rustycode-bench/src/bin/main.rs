#![allow(
    clippy::branches_sharing_code,
    clippy::expect_used,
    clippy::needless_pass_by_value,
    clippy::redundant_clone,
    clippy::uninlined_format_args,
    clippy::unnecessary_debug_formatting
)]
use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

use rustycode_bench::{
    config::{create_agent, AgentConfig, BenchConfig},
    dataset::DatasetRegistry,
    history::{format_diff, HistoryStore},
    registry::RegistryDownloader,
    swebench::{run_evaluation, run_swebench, EvalConfig, SweBenchConfig},
    AgentFactory, BenchmarkResults, CodeAgentConfig, DockerRunner, DockerRunnerConfig,
    NativeRunner, NativeRunnerConfig, ResolvedTask, RetryConfig,
};

// CLI definition

#[derive(Parser)]
#[command(
    name = "rtk-bench",
    about = "Benchmark runner for agent evaluation (Harbor-compatible)",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
enum Commands {
    /// Run a benchmark (Harbor-compatible)
    Run {
        /// Load configuration from a JSON or TOML file (Harbor config equivalent)
        #[arg(long)]
        config: Option<PathBuf>,

        /// Dataset reference: local path or registry ref (e.g. "terminal-bench@2.0")
        #[arg(value_name = "DATASET")]
        dataset: Option<String>,

        /// Dataset reference (alternative to positional)
        #[arg(long)]
        dataset_ref: Option<String>,

        /// Run only specific tasks by name (comma-separated, substring match)
        #[arg(long)]
        task: Option<String>,

        /// Limit number of tasks to run (for quick testing)
        #[arg(long)]
        max_tasks: Option<usize>,

        /// Agent to use: oracle, code, nop
        #[arg(long, default_value = "oracle")]
        agent: String,

        /// Model for the code agent (e.g. "claude-sonnet-4-6")
        #[arg(long, default_value = "auto")]
        model: String,

        /// Override provider auto-detection: anthropic, openai
        #[arg(long)]
        provider: Option<String>,

        /// Number of concurrent trials
        #[arg(long, default_value_t = 1)]
        n_concurrent: usize,

        /// Execution environment: native, docker, bollard
        #[arg(long, default_value = "native")]
        env: String,

        /// Force rebuild container images
        #[arg(long)]
        force_build: bool,

        /// Skip container cleanup after trials
        #[arg(long)]
        no_cleanup: bool,

        /// Human-readable job name
        #[arg(long)]
        job_name: Option<String>,

        /// Number of retries per trial on infrastructure failure
        #[arg(long, default_value_t = 2)]
        retry: usize,

        /// Per-trial wall-clock timeout in seconds (0 = auto from task config)
        #[arg(long, default_value_t = 0)]
        timeout: u64,

        /// Output format: pretty (default), json, csv, markdown, summary
        #[arg(long, default_value = "pretty", value_parser = ["pretty", "json", "csv", "markdown", "summary"])]
        output: String,

        /// Save a formatted report to file (format derived from --output)
        #[arg(long)]
        report: Option<String>,
    },

    /// List datasets and tasks
    List {
        /// Optional dataset reference to inspect
        dataset_ref: Option<String>,

        /// Fetch dataset list from remote registry
        #[arg(long)]
        remote: bool,

        /// Show detailed task information
        #[arg(short, long)]
        verbose: bool,
    },

    /// Verify a single task's structure
    Verify {
        /// Path to the task directory
        task_dir: PathBuf,
    },

    /// List available agent types
    Agents,

    /// View benchmark run history and compare runs
    History {
        #[command(subcommand)]
        action: HistoryAction,
    },

    /// Run SWE-bench evaluation (honest, no-tricks)
    Swebench {
        /// Path to SWE-bench instances JSON/JSONL file
        #[arg(long)]
        instances: PathBuf,

        /// Output path for predictions
        #[arg(long, default_value = "predictions.json")]
        output: PathBuf,

        /// Model to use (e.g. claude-sonnet-4-6)
        #[arg(long, default_value = "claude-sonnet-4-6")]
        model: String,

        /// LLM provider: anthropic, openai
        #[arg(long, default_value = "anthropic")]
        provider: String,

        /// Max tool-use turns per instance
        #[arg(long, default_value_t = 30)]
        max_turns: usize,

        /// Max tokens per LLM response
        #[arg(long, default_value_t = 16_384)]
        max_tokens: u32,

        /// Timeout per instance in seconds
        #[arg(long, default_value_t = 600)]
        timeout: u64,

        /// Specific instance IDs to run (comma-separated)
        #[arg(long)]
        instance_ids: Option<String>,

        /// Output format: json or jsonl
        #[arg(long, default_value = "json")]
        format: String,

        /// Working directory for cloned repos
        #[arg(long, default_value = "swebench-work")]
        work_dir: PathBuf,
    },

    /// Evaluate SWE-bench predictions (apply patches + run tests)
    Evaluate {
        /// Path to predictions JSON/JSONL file
        #[arg(long)]
        predictions: PathBuf,

        /// Path to instances JSON/JSONL file
        #[arg(long)]
        instances: PathBuf,

        /// Specific instance IDs to evaluate (comma-separated)
        #[arg(long)]
        instance_ids: Option<String>,

        /// Per-instance test timeout in seconds
        #[arg(long, default_value_t = 300)]
        timeout: u64,

        /// Max `PASS_TO_PASS` tests to run (0 = all)
        #[arg(long, default_value_t = 50)]
        max_pass_to_pass: usize,

        /// Working directory containing cloned repos
        #[arg(long, default_value = "swebench-work")]
        work_dir: PathBuf,

        /// Output path for evaluation results JSON
        #[arg(long)]
        output: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum HistoryAction {
    /// List past benchmark runs
    List {
        /// Filter by dataset path
        #[arg(long)]
        dataset: Option<String>,
    },

    /// Show details of a specific run
    Show {
        /// Run ID to display
        run_id: String,
    },

    /// Compare two runs
    Diff {
        /// Baseline run ID
        baseline: String,

        /// Comparison run ID
        comparison: String,
    },
}

// Entry point

#[tokio::main]
#[allow(clippy::too_many_lines)]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new("info"))
        .init();

    match cli.command {
        Commands::Run {
            config,
            dataset,
            dataset_ref,
            task: task_filter,
            max_tasks,
            agent,
            model,
            provider,
            n_concurrent,
            env: run_env,
            force_build,
            no_cleanup,
            job_name,
            retry,
            timeout,
            output,
            report,
        } => {
            let dataset_ref = dataset.or(dataset_ref);
            let cfg = if let Some(config_path) = &config {
                let file_cfg = BenchConfig::load(config_path)?;
                file_cfg.merge_cli(
                    dataset_ref,
                    if agent == "oracle" { None } else { Some(agent) },
                    if model == "auto" { None } else { Some(model) },
                    if run_env == "native" {
                        None
                    } else {
                        Some(run_env)
                    },
                    if n_concurrent == 1 {
                        None
                    } else {
                        Some(n_concurrent)
                    },
                    if timeout == 0 { None } else { Some(timeout) },
                    task_filter,
                    max_tasks,
                    job_name,
                    if force_build { Some(true) } else { None },
                    if no_cleanup { Some(false) } else { None },
                    if retry == 2 { None } else { Some(retry) },
                    if output == "pretty" {
                        None
                    } else {
                        Some(output)
                    },
                )
            } else {
                BenchConfig {
                    dataset: dataset_ref.unwrap_or_else(|| ".".to_string()),
                    agent: AgentConfig {
                        name: agent,
                        model,
                        ..Default::default()
                    },
                    env: run_env,
                    n_concurrent,
                    timeout,
                    task_filter,
                    max_tasks,
                    job_name,
                    force_build,
                    cleanup: !no_cleanup,
                    retry: RetryConfig {
                        max_retries: retry,
                        ..Default::default()
                    },
                    output,
                    provider,
                    ..Default::default()
                }
            };
            run_benchmark(&cfg, report.as_deref()).await
        }

        Commands::List {
            dataset_ref,
            remote,
            verbose,
        } => list_tasks(dataset_ref.as_deref(), remote, verbose).await,

        Commands::Verify { task_dir } => verify_task(task_dir),

        Commands::Agents => {
            list_agents();
            Ok(())
        }

        Commands::History { action } => handle_history(action),

        Commands::Swebench {
            instances,
            output,
            model,
            provider,
            max_turns,
            max_tokens,
            timeout,
            instance_ids,
            format,
            work_dir,
        } => {
            let ids = instance_ids.map(|s| {
                s.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            });
            let config = SweBenchConfig {
                instances_path: instances,
                output_path: output,
                format,
                model_name: model.clone(),
                instance_ids: ids,
                work_dir,
                agent_config: CodeAgentConfig {
                    model,
                    provider,
                    max_turns,
                    max_tokens,
                    ..Default::default()
                },
                timeout_secs: timeout,
            };
            run_swebench(config).await?;
            Ok(())
        }

        Commands::Evaluate {
            predictions,
            instances,
            instance_ids,
            timeout,
            max_pass_to_pass,
            work_dir,
            output,
        } => {
            let ids = instance_ids.map(|s| {
                s.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            });
            let config = EvalConfig {
                predictions_path: predictions,
                instances_path: instances,
                work_dir,
                instance_ids: ids,
                test_timeout_secs: timeout,
                max_pass_to_pass,
                output_path: output,
            };
            run_evaluation(config).await?;
            Ok(())
        }
    }
}

// run subcommand

async fn run_benchmark(cfg: &BenchConfig, report_path: Option<&str>) -> Result<()> {
    let agent_name = &cfg.agent.name;
    let model = &cfg.agent.model;

    if agent_name == "code" {
        let has_key = std::env::var("ANTHROPIC_API_KEY").is_ok()
            || std::env::var("OPENAI_API_KEY").is_ok()
            || std::env::var("GOOGLE_API_KEY").is_ok();
        if !has_key && model == "auto" {
            bail!("No API key. Set ANTHROPIC_API_KEY, OPENAI_API_KEY, or GOOGLE_API_KEY");
        }
    }

    let job_name = cfg.job_name.clone().unwrap_or_else(|| {
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        // Include short random suffix so parallel launches get unique temp dirs
        let suffix = &uuid::Uuid::new_v4().to_string()[..8];
        format!("bench_{agent_name}_{timestamp}_{suffix}")
    });

    tracing::info!(
        "Starting benchmark: {job_name} (agent={agent_name}, env={}, concurrent={}, retry={})",
        cfg.env,
        cfg.n_concurrent,
        cfg.retry.max_retries
    );

    let dataset_path = resolve_dataset(&cfg.dataset).await?;

    let mut tasks = ResolvedTask::discover(&dataset_path)
        .with_context(|| format!("Failed to discover tasks in {}", dataset_path.display()))?;

    if tasks.is_empty() {
        bail!("No tasks found in {}", dataset_path.display());
    }

    if let Some(filter) = &cfg.task_filter {
        let patterns: Vec<&str> = filter.split(',').map(str::trim).collect();
        let before = tasks.len();
        tasks.retain(|t| patterns.iter().any(|p| t.name.contains(p)));
        if tasks.is_empty() {
            bail!("No tasks match filter: {filter}");
        }
        tracing::info!("Task filter '{filter}': {before} -> {} tasks", tasks.len());
    }

    tracing::info!("Running {} tasks", tasks.len());

    if let Some(max) = cfg.max_tasks {
        if tasks.len() > max {
            tracing::info!("Limiting to first {max} of {} tasks", tasks.len());
            tasks.truncate(max);
        }
    }

    let provider_override = cfg.provider.clone();
    let agent_factory: AgentFactory = Box::new(move |name: &str, mdl: &str, sol_dir: PathBuf| {
        create_agent(name, mdl, sol_dir, provider_override.as_deref())
    });

    let results = match cfg.env.as_str() {
        "docker" => {
            let runner = DockerRunner::new(DockerRunnerConfig {
                agent_name: agent_name.clone(),
                model: model.clone(),
                n_concurrent: cfg.n_concurrent,
                job_name: job_name.clone(),
                force_build: cfg.force_build,
                cleanup: cfg.cleanup,
                retry_config: cfg.retry.clone(),
            });
            runner.run(&tasks, &dataset_path, agent_factory).await
        }
        "native" => {
            let runner = NativeRunner::new(NativeRunnerConfig {
                agent_name: agent_name.clone(),
                model: model.clone(),
                n_concurrent: cfg.n_concurrent,
                job_name: job_name.clone(),
                retry_config: cfg.retry.clone(),
                per_trial_timeout: cfg.timeout,
            });
            runner.run(&tasks, &dataset_path, agent_factory).await
        }
        "bollard" => {
            // Reuse DockerRunner for now — BollardEnvironment is available
            // for direct use; a dedicated BollardRunner can be added later.
            let runner = DockerRunner::new(DockerRunnerConfig {
                agent_name: agent_name.clone(),
                model: model.clone(),
                n_concurrent: cfg.n_concurrent,
                job_name: job_name.clone(),
                force_build: cfg.force_build,
                cleanup: cfg.cleanup,
                retry_config: cfg.retry.clone(),
            });
            runner.run(&tasks, &dataset_path, agent_factory).await
        }
        other => bail!("Unknown environment: '{other}'. Use 'native', 'docker', or 'bollard'"),
    }?;

    print_results(&results, &cfg.output);

    if let Some(path) = report_path {
        save_report(&results, path, &cfg.output)?;
    }

    Ok(())
}

/// Resolve a dataset reference to a local path, downloading if needed.
async fn resolve_dataset(dataset_ref: &str) -> Result<PathBuf> {
    let local = PathBuf::from(dataset_ref);
    if local.exists() {
        return Ok(local);
    }

    let registry = DatasetRegistry::new();
    if let Ok(p) = registry.resolve(dataset_ref) {
        return Ok(p);
    }

    tracing::info!("Resolving dataset: {dataset_ref} (may download)...");
    let downloader = RegistryDownloader::new();
    downloader
        .resolve(dataset_ref)
        .await
        .with_context(|| format!("Failed to resolve dataset '{dataset_ref}'"))
}

// list subcommand

async fn list_tasks(dataset_ref: Option<&str>, remote: bool, verbose: bool) -> Result<()> {
    if remote {
        list_remote_datasets(verbose).await?;
        return Ok(());
    }

    let registry = DatasetRegistry::new();

    if let Some(reference) = dataset_ref {
        let path = if PathBuf::from(reference).exists() {
            PathBuf::from(reference)
        } else if let Ok(p) = registry.resolve(reference) {
            p
        } else {
            let downloader = RegistryDownloader::new();
            downloader
                .resolve(reference)
                .await
                .with_context(|| format!("Dataset not found: {reference}"))?
        };

        let tasks = ResolvedTask::discover(&path)?;

        println!("\n=== Tasks in {} ===", path.display());
        println!("Total: {}\n", tasks.len());

        for task in &tasks {
            if verbose {
                println!("- {}", task.name);
                println!(
                    "  Category: {}",
                    if task.config.metadata.category.is_empty() {
                        "(none)"
                    } else {
                        &task.config.metadata.category
                    }
                );
                println!(
                    "  Difficulty: {}",
                    if task.config.metadata.difficulty.is_empty() {
                        "(none)"
                    } else {
                        &task.config.metadata.difficulty
                    }
                );
                println!(
                    "  Verifier timeout: {:.0}s",
                    task.config.verifier.timeout_sec
                );
                println!();
            } else {
                println!("- {}", task.name);
            }
        }
    } else {
        list_local_tasks(&registry, verbose);
    }

    Ok(())
}

fn list_local_tasks(registry: &DatasetRegistry, verbose: bool) {
    let datasets = registry.list_datasets();
    let downloader = RegistryDownloader::new();
    let cached = downloader.list_cached();

    if datasets.is_empty() && cached.is_empty() {
        println!("No datasets found locally.");
        println!("\nUsage:");
        println!("  rtk-bench run --dataset terminal-bench@2.0 --agent oracle");
        println!("  rtk-bench list --remote    # fetch from registry");
        return;
    }

    println!("\n=== Local Tasks ===\n");

    let mut all_tasks: Vec<(String, String, String)> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let all_paths: Vec<PathBuf> = datasets
        .iter()
        .map(|d| d.path.clone())
        .chain(cached.iter().map(|d| d.path.clone()))
        .collect();

    for ds_path in &all_paths {
        if let Ok(tasks) = ResolvedTask::discover(ds_path) {
            for task in tasks {
                if seen.insert(task.name.clone()) {
                    all_tasks.push((
                        task.name.clone(),
                        task.config.metadata.category.clone(),
                        task.config.metadata.difficulty.clone(),
                    ));
                }
            }
        }
    }

    all_tasks.sort_by(|a, b| a.0.cmp(&b.0));

    if verbose {
        for (name, category, difficulty) in &all_tasks {
            println!("- {name}");
            if !category.is_empty() {
                println!("  category: {category}");
            }
            if !difficulty.is_empty() {
                println!("  difficulty: {difficulty}");
            }
        }
    } else {
        for (name, _, _) in &all_tasks {
            println!("- {name}");
        }
    }

    println!("\nTotal: {} tasks", all_tasks.len());
}

async fn list_remote_datasets(verbose: bool) -> Result<()> {
    let downloader = RegistryDownloader::new();

    println!("Fetching remote datasets...\n");

    let datasets = downloader.list_remote().await?;

    if datasets.is_empty() {
        println!("No remote datasets found.");
        return Ok(());
    }

    println!("=== Remote Datasets ===\n");
    for ds in &datasets {
        println!("- {} v{}", ds.name, ds.version);
        println!("  {}", ds.description);
        if verbose {
            println!("  Tasks: {}", ds.tasks.len());
        }
        println!();
    }

    Ok(())
}

// verify subcommand

fn verify_task(task_dir: PathBuf) -> Result<()> {
    let task = ResolvedTask::from_dir(&task_dir)
        .with_context(|| format!("Failed to load task from {}", task_dir.display()))?;

    println!("\n=== Task: {} ===", task.name);
    println!(
        "Category: {}",
        if task.config.metadata.category.is_empty() {
            "(none)"
        } else {
            &task.config.metadata.category
        }
    );
    println!(
        "Difficulty: {}",
        if task.config.metadata.difficulty.is_empty() {
            "(none)"
        } else {
            &task.config.metadata.difficulty
        }
    );

    let has_instructions = !task.instruction.is_empty();
    println!(
        "\nInstructions: {}",
        if has_instructions { "OK" } else { "MISSING" }
    );

    let has_dockerfile = task.environment_dir.join("Dockerfile").exists();
    println!(
        "Dockerfile: {}",
        if has_dockerfile {
            "OK"
        } else {
            "not present (native mode)"
        }
    );

    let has_tests = task.tests_dir.join("test.sh").exists();
    println!("Tests: {}", if has_tests { "OK" } else { "MISSING" });

    let has_solution = task.solution_dir.exists();
    println!(
        "Solution: {}",
        if has_solution { "OK" } else { "NOT PROVIDED" }
    );

    let valid = has_instructions && has_tests;
    println!("\nStatus: {}", if valid { "VALID" } else { "INCOMPLETE" });

    if !valid {
        bail!("Task is incomplete");
    }

    Ok(())
}

// agents subcommand

fn list_agents() {
    println!("\n=== Available Agents ===\n");

    let agents: Vec<(&str, &str)> = vec![
        (
            "oracle",
            "Runs the pre-written solution.sh (infrastructure validation)",
        ),
        (
            "code",
            "Uses an LLM to solve the task with bash tool access",
        ),
        ("nop", "Does nothing (infrastructure smoke test)"),
    ];

    for (name, description) in &agents {
        println!("- {name}: {description}");
    }

    println!("\nUsage:");
    println!("  rtk-bench run --dataset terminal-bench@2.0 --agent oracle");
    println!("  rtk-bench run --dataset ./my-tasks --agent code --model claude-sonnet-4-6");
}

// history subcommand

fn handle_history(action: HistoryAction) -> Result<()> {
    let base_dir = std::env::current_dir()?;
    let store = HistoryStore::new(base_dir);

    match action {
        HistoryAction::List { dataset: _ } => {
            let runs = store.list()?;
            if runs.is_empty() {
                println!("No historical runs found.");
                println!("Run a benchmark first: rtk-bench run --dataset <path>");
                return Ok(());
            }

            println!("\n=== Historical Runs ===\n");
            for run in &runs {
                println!(
                    "- {} | {} | {} tasks | accuracy {:.1}% | reward {:.3}",
                    run.run_id,
                    run.timestamp,
                    run.results.total,
                    run.results.accuracy * 100.0,
                    run.results.mean_reward,
                );
            }
            println!("\nTotal: {} runs", runs.len());
        }

        HistoryAction::Show { run_id } => {
            let run = store.load(&run_id)?;
            println!("\n=== Run: {} ===", run.run_id);
            println!("Job: {}", run.job_name);
            println!("Timestamp: {}", run.timestamp);
            println!("Agent: {}", run.config.agent.name);
            println!("Environment: {}", run.config.env);
            println!("Dataset: {}", run.config.dataset);
            println!("\nResults:");
            println!("  Total: {}", run.results.total);
            println!("  Passed: {}", run.results.passed);
            println!("  Failed: {}", run.results.failed);
            println!("  Accuracy: {:.1}%", run.results.accuracy * 100.0);
            println!("  Mean reward: {:.3}", run.results.mean_reward);

            if !run.results.task_results.is_empty() {
                println!("\nTask breakdown:");
                for tr in &run.results.task_results {
                    let status = if tr.passed { "PASS" } else { "FAIL" };
                    println!("  - {}: {} ({:.2})", tr.task_name, status, tr.reward);
                }
            }
        }

        HistoryAction::Diff {
            baseline,
            comparison,
        } => {
            let diff = store.diff(&baseline, &comparison)?;
            print!("{}", format_diff(&diff));
        }
    }

    Ok(())
}

// Result printing

fn print_results(results: &BenchmarkResults, format: &str) {
    let formatter: Box<dyn rustycode_bench::report::ReportFormatter> = match format {
        "json" => Box::new(rustycode_bench::report::JsonFormatter),
        "csv" => Box::new(rustycode_bench::report::CsvFormatter),
        "markdown" => Box::new(rustycode_bench::report::MarkdownFormatter),
        "summary" => {
            println!("{}", results.summary());
            return;
        }
        _ => Box::new(rustycode_bench::report::PrettyFormatter),
    };
    print!("{}", formatter.format_results(results));
}

fn save_report(results: &BenchmarkResults, path: &str, format: &str) -> anyhow::Result<()> {
    let formatter: Box<dyn rustycode_bench::report::ReportFormatter> = match format {
        "json" => Box::new(rustycode_bench::report::JsonFormatter),
        "csv" => Box::new(rustycode_bench::report::CsvFormatter),
        "markdown" => Box::new(rustycode_bench::report::MarkdownFormatter),
        _ => Box::new(rustycode_bench::report::PrettyFormatter),
    };
    let content = formatter.format_results(results);
    std::fs::write(path, content).with_context(|| format!("Failed to write report to {path}"))?;
    tracing::info!("Report saved to {path}");
    Ok(())
}
