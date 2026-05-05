#![allow(
    clippy::bool_to_int_with_if,
    clippy::expect_used,
    clippy::items_after_statements,
    clippy::map_unwrap_or,
    clippy::needless_continue,
    clippy::option_if_let_else,
    clippy::redundant_clone,
    clippy::redundant_closure_for_method_calls,
    clippy::redundant_else,
    clippy::semicolon_if_nothing_returned,
    clippy::too_many_lines,
    clippy::uninlined_format_args
)]

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use rustycode_cli::prompt::PromptConfig;
use rustycode_cli::Prompt;
use rustycode_config::paths::RustyCodePath;
use rustycode_protocol::SessionId;
use rustycode_runtime::AsyncRuntime;
use rustycode_storage::GitRewindSnapshot;
use std::path::PathBuf;
use std::time::Duration;
use tracing_subscriber::filter::LevelFilter;

mod server;
use commands::cli_args::*;
use commands::harness_cmd;
use commands::history_cmd;
use commands::provider_command::{self as provider_cmd, ProviderCommand};
use commands::skills_cmd;
use rustycode_cli::commands;

#[derive(Debug, Parser)]
#[command(
    name = "rustycode",
    version,
    about = "Rust-native coding agent workspace",
    subcommand_negates_reqs = true
)]
struct Cli {
    /// Task to execute directly (equivalent to `rustycode run <task>`).
    /// Use quotes for multi-word tasks: rustycode "fix the bug"
    #[arg(value_name = "TASK", hide_possible_values = true)]
    task: Option<String>,

    /// Automatically answer yes to all prompts (non-interactive mode)
    #[arg(long, global = true)]
    yes: bool,

    /// Enable or disable colored output
    #[arg(long, global = true, default_value = "auto")]
    color: String,

    /// Output format: human (default) or json
    #[arg(long, global = true, default_value = "human")]
    format: String,

    /// Override the configured LLM model for this invocation
    #[arg(long, global = true)]
    model: Option<String>,

    /// Override the effort level for this invocation (low, medium, high, xhigh, max)
    #[arg(long, global = true)]
    effort: Option<String>,

    /// Enable verbose logging
    #[arg(long, global = true)]
    verbose: bool,

    /// Enable debug logging (includes verbose)
    #[arg(long, global = true)]
    debug: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    Doctor,
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    Context {
        prompt: String,
    },
    Run {
        /// Task prompt to execute
        prompt: String,
        /// Auto mode: execute without TUI, output results and exit
        #[arg(long)]
        auto: bool,
        /// Output format: human (default) or json
        #[arg(long, default_value = "human")]
        format: String,
        /// Working mode (affects prompts and behavior):
        ///   auto       - Automatically detect intent and select mode (default)
        ///   code       - Implementation and feature development
        ///   debug      - Troubleshooting and issue diagnosis
        ///   ask        - Quick questions and information retrieval
        ///   orchestrate - Multi-agent coordination and complex workflows
        ///   plan       - Planning and architecture design
        ///   test       - Test-driven development and testing
        #[arg(long, default_value = "auto", value_name = "MODE")]
        mode: String,
        /// Use Adaptive Structured Thinking (AST) pipeline for complex task decomposition
        #[arg(long)]
        use_ast: bool,
        /// Override task complexity classification (trivial, moderate, complex)
        #[arg(long, value_name = "LEVEL")]
        ast_complexity: Option<String>,
        /// JSON schema for structured output (string or @file path).
        /// When set, the response is validated against this schema.
        #[arg(long, value_name = "SCHEMA")]
        json_schema: Option<String>,
    },
    Tools {
        #[command(subcommand)]
        command: ToolsCommand,
    },
    Sessions {
        #[command(subcommand)]
        command: SessionsCommand,
    },
    Events {
        #[command(subcommand)]
        command: EventsCommand,
    },
    /// Plan mode: create, list, show, approve, or reject plans.
    Plan {
        #[command(subcommand)]
        command: PlanCommand,
    },
    /// Agent mode: autonomous task execution with LLM reasoning.
    Agent {
        #[command(subcommand)]
        command: AgentCommand,
    },
    /// Harness mode: long-running agent framework with progress persistence
    Harness {
        #[command(subcommand)]
        command: HarnessCommand,
    },
    /// OMO multi-agent orchestration for comprehensive code analysis.
    Omo {
        #[command(subcommand)]
        command: OmoCommand,
    },
    /// Git worktree management for isolated development.
    Worktree {
        #[command(subcommand)]
        command: WorktreeCommand,
    },
    /// Provider and model management for LLM selection.
    Provider {
        #[command(subcommand)]
        command: ProviderCommand,
    },
    /// Conversation history management (list, search, show, export).
    History {
        #[command(subcommand)]
        command: HistoryCommand,
    },
    /// Skills management (list, run, create, validate).
    Skills {
        #[command(subcommand)]
        command: SkillsCommand,
    },
    /// Team learnings management (show, add, remove project memory).
    Learnings {
        #[command(subcommand)]
        command: LearningsCommand,
    },
    /// Launch the interactive TUI.
    Tui {
        /// Force the configuration wizard to run (even if config exists)
        #[arg(long)]
        reconfigure: bool,
        /// Resume most recent session
        #[arg(long)]
        resume: bool,
        /// Override the AI model for this session
        #[arg(long, value_name = "MODEL")]
        model: Option<String>,
        /// Override workspace directory (default: current directory)
        #[arg(short, long, value_name = "PATH")]
        workspace: Option<PathBuf>,
    },
    /// Launch the web-native interface.
    Web {
        #[command(subcommand)]
        command: WebCommand,
    },
    /// Serve the web UI and API server.
    Serve {
        /// Port to listen on (default: 3000)
        #[arg(short, long, default_value = "3000")]
        port: u16,
        /// Directory to serve static files from
        #[arg(short, long)]
        dir: Option<String>,
    },
    /// Run SWE-bench evaluation (load instances, generate predictions).
    Swebench {
        #[command(flatten)]
        args: SweBenchCliArgs,
    },
    /// Rewind a repository to a checkpoint (git hash) and optionally restore specific files.
    Checkpoint {
        /// Path to the repository (defaults to current directory)
        #[arg(short, long, default_value = ".")]
        repo: String,
        /// Git hash to rewind to
        git_hash: String,
        /// Comma-separated list of files to restore (optional)
        #[arg(short = 'f', long, value_delimiter = ',')]
        files: Option<Vec<String>>,
        /// Force the restore without prompting (destructive)
        #[arg(long)]
        force: bool,
        /// Dry-run: preview changes without applying them
        #[arg(long)]
        dry_run: bool,
    },

    /// Benchmark runner for agent evaluation (Terminal Bench compatible).
    Bench {
        #[command(subcommand)]
        command: BenchCommand,
    },
    /// AST (Adaptive Structured Thinking) pipeline commands.
    Ast {
        #[command(subcommand)]
        command: AstCommand,
    },
}

fn main() -> Result<()> {
    // Build tokio runtime with optimized configuration for CPU-bound workloads
    // Uses number of CPU cores for maximum parallelism in tool execution
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(num_cpus::get())
        .thread_name_fn(|| {
            static ATOMIC_ID: std::sync::atomic::AtomicUsize =
                std::sync::atomic::AtomicUsize::new(0);
            let id = ATOMIC_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            format!("main-runtime-worker-{}", id)
        })
        .enable_all()
        .build()
        .map_err(|e| anyhow::anyhow!("failed to build tokio runtime: {}", e))?;
    rt.block_on(async_main())
}

fn resolve_config_key(key: &str) -> String {
    match key {
        "log_level" => "advanced.log_level".to_string(),
        "telemetry_enabled" => "advanced.telemetry_enabled".to_string(),
        "cache_enabled" => "advanced.cache_enabled".to_string(),
        "confidence_threshold" => "task_routing.confidence_threshold".to_string(),
        "max_clarifying_questions" => "task_routing.max_clarifying_questions".to_string(),
        "max_research_passes" => "task_routing.max_research_passes".to_string(),
        other => other.to_string(),
    }
}

fn get_value_at_path<'a>(
    value: &'a serde_json::Value,
    path: &str,
) -> Option<&'a serde_json::Value> {
    let mut current = value;
    for segment in path.split('.') {
        current = current.get(segment)?;
    }
    Some(current)
}

fn set_value_at_path(
    value: &mut serde_json::Value,
    path: &str,
    new_value: serde_json::Value,
) -> Result<()> {
    let mut segments = path.split('.').filter(|s| !s.is_empty()).peekable();
    if segments.peek().is_none() {
        anyhow::bail!("Empty config path");
    }

    let mut current = value;
    while let Some(segment) = segments.next() {
        if segments.peek().is_none() {
            if !current.is_object() {
                *current = serde_json::json!({});
            }
            current
                .as_object_mut()
                .ok_or_else(|| {
                    anyhow::anyhow!("Failed to access config path segment '{}'", segment)
                })?
                .insert(segment.to_string(), new_value);
            return Ok(());
        }

        if !current.get(segment).is_some_and(|v| v.is_object()) {
            if !current.is_object() {
                *current = serde_json::json!({});
            }
            current[segment] = serde_json::json!({});
        }

        current = current
            .get_mut(segment)
            .ok_or_else(|| anyhow::anyhow!("Failed to access config path segment '{}'", segment))?;
    }

    Ok(())
}

fn config_value_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Array(arr) => serde_json::to_string(arr).unwrap_or_default(),
        serde_json::Value::Object(obj) => serde_json::to_string(obj).unwrap_or_default(),
        serde_json::Value::Null => String::new(),
    }
}

async fn async_main() -> Result<()> {
    // Redirect tracing output to log file instead of stderr to avoid screen pollution
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let log_dir = PathBuf::from(home).join(".rustycode");
    let log_path = log_dir.join("debug.log");
    let _ = std::fs::create_dir_all(&log_dir);

    let cli = Cli::parse();

    // Try to open log file; fall back to stderr if unavailable (non-fatal)
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path);

    let log_level = if cli.debug {
        LevelFilter::DEBUG
    } else if cli.verbose {
        LevelFilter::INFO
    } else {
        LevelFilter::WARN
    };

    match log_file {
        Ok(file) => {
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::from_default_env()
                        .add_directive(log_level.into()),
                )
                .with_target(false)
                .without_time()
                .with_writer(std::sync::Mutex::new(file))
                .init();
        }
        Err(_) => {
            // Fallback: write to stderr (better than crashing)
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::from_default_env()
                        .add_directive(log_level.into()),
                )
                .with_target(false)
                .without_time()
                .init();
        }
    }

    // Resolve effective command:
    //   - explicit subcommand → use it
    //   - positional TASK arg → treat as Run
    //   - nothing → default to Tui
    // Note: stdin reading removed - piping requires different handling via shell
    let command = if let Some(cmd) = cli.command {
        cmd
    } else if let Some(ref task) = cli.task {
        // Check if task looks like a misspelled subcommand
        if let Some(suggestion) = suggest_similar_subcommand(task) {
            eprintln!(
                "Note: '{}' is not a subcommand. Did you mean '{}'?",
                task, suggestion
            );
            eprintln!("Starting task in agent mode...\n");
        }
        Command::Run {
            prompt: task.clone(),
            auto: false,
            format: cli.format.clone(),
            mode: "auto".to_string(),
            use_ast: false,
            ast_complexity: None,
            json_schema: None,
        }
    } else {
        Command::Tui {
            reconfigure: false,
            resume: false,
            model: None,
            workspace: None,
        }
    };

    let cwd = std::env::current_dir()?;

    // Apply model override via env var (read by LLM provider config loader)
    if let Some(ref model) = cli.model {
        std::env::set_var("RUSTYCODE_MODEL_OVERRIDE", model);
    }

    if let Some(ref effort) = cli.effort {
        if rustycode_llm::provider::EffortLevel::try_from(effort.as_str()).is_err() {
            eprintln!(
                "error: invalid effort level '{}'. Valid values: low, medium, high, xhigh, max",
                effort
            );
            std::process::exit(1);
        }
        std::env::set_var("RUSTYCODE_EFFORT_OVERRIDE", effort);
    }

    // Configure colored output
    match cli.color.as_str() {
        "always" => colored::control::set_override(true),
        "never" => colored::control::set_override(false),
        "auto" => {
            // Let colored crate detect automatically
            colored::control::unset_override();
        }
        _ => {
            eprintln!(
                "Invalid color option: {}. Use 'always', 'never', or 'auto'.",
                cli.color
            );
            std::process::exit(1);
        }
    }

    // Configure global prompt settings
    if cli.yes {
        PromptConfig::set_global_yes_enabled(true);
    }

    // TUI takes over the terminal — must run on a fresh thread to avoid nesting tokio runtimes
    if let Command::Tui {
        reconfigure,
        resume,
        model,
        workspace,
    } = command
    {
        let cwd = workspace
            .map(|p| p.canonicalize().context("--workspace path does not exist"))
            .transpose()?
            .unwrap_or(cwd);
        // Apply model override via env var (read by LLM provider config loader)
        if let Some(ref m) = model {
            std::env::set_var("RUSTYCODE_MODEL_OVERRIDE", m);
        }
        return std::thread::spawn(move || {
            rustycode_tui::run(cwd, reconfigure, resume).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to start TUI: {}\nHint: run this command in an interactive terminal (not piped/redirected).",
                    e
                )
            })
        })
        .join()
        .map_err(|_| anyhow::anyhow!("TUI thread panicked"))?;
    }

    let runtime = AsyncRuntime::load(&cwd).await?;
    match command {
        Command::Tui { .. } => {
            // Already handled above
            unreachable!();
        }
        Command::Web { command } => match command {
            WebCommand::Start { port } => {
                crate::commands::web_start::start_web_server(port).await?;
            }
            _ => {
                anyhow::bail!("Unknown web command.");
            }
        },
        Command::Serve { port, dir } => server::serve_web(port, dir).await?,

        Command::Checkpoint {
            repo,
            git_hash,
            files,
            force,
            dry_run,
        } => {
            let repo_path = PathBuf::from(repo);

            let snapshot = GitRewindSnapshot {
                git_hash: git_hash.clone(),
                files: files.clone().unwrap_or_default(),
            };

            if dry_run {
                // Preview changes and exit
                let summary = runtime
                    .preview_rewind_repo(&repo_path, &snapshot)
                    .await
                    .with_context(|| {
                        format!("Failed to preview rewind for {}", repo_path.display())
                    })?;
                println!(
                    "Dry-run preview for {}:\n\n{}",
                    repo_path.display(),
                    summary
                );
                return Ok(());
            }

            // Check for uncommitted changes (blocking) and prompt unless forced or global yes
            let repo_clone = repo_path.clone();
            let is_dirty = tokio::task::spawn_blocking(move || {
                rustycode_storage::repo_has_uncommitted_changes(&repo_clone)
            })
            .await
            .map_err(|e| anyhow::anyhow!(e))??;

            if is_dirty && !force {
                // Use CLI prompt for confirmation (respects --yes/global prompt config)
                use rustycode_cli::prompt::Confirm;
                let message = format!(
                    "Repository {} has uncommitted changes. Proceed and discard local changes?",
                    repo_path.display()
                );
                let confirmed = Confirm::new(message).with_default(false).prompt()?;
                if !confirmed {
                    println!("Aborted checkpoint restore by user.");
                    return Ok(());
                }
            }

            runtime
                .rewind_repo(&repo_path, &snapshot)
                .await
                .with_context(|| format!("Failed to rewind repository {}", repo_path.display()))?;
            println!("Rewound {} to {}", repo_path.display(), snapshot.git_hash);
        }

        Command::Swebench { args } => {
            use commands::swebench_command::{run_swebench, SweBenchArgs};
            let swebench_args = SweBenchArgs {
                instances: args.instances,
                output: args.output,
                budget: args.budget,
                parallel: args.parallel,
                instance_ids: args.instance_ids,
                format: args.format,
            };
            run_swebench(swebench_args).await?;
        }

        Command::Doctor => {
            let report = runtime.doctor(&cwd).await?;
            // Redact API keys before printing
            let mut json = serde_json::to_value(&report)?;
            if let Some(config) = json.get_mut("config") {
                *config = runtime.config().redacted_for_display();
            }
            if cli.format == "json" {
                println!("{}", serde_json::to_string_pretty(&json)?);
            } else {
                println!("RustyCode Doctor");
                println!("================");
                if let Some(git) = json.get("git") {
                    println!();
                    println!("Git:");
                    if let Some(branch) = git.get("branch").and_then(|v| v.as_str()) {
                        println!("  Branch:  {}", branch);
                    }
                    if let Some(root) = git.get("root").and_then(|v| v.as_str()) {
                        println!("  Root:    {}", root);
                    }
                    let dirty = git.get("dirty").and_then(|v| v.as_bool()).unwrap_or(false);
                    println!("  Dirty:   {}", if dirty { "yes" } else { "no" });
                }
                if let Some(config) = json.get("config") {
                    println!();
                    println!("Config:");
                    if let Some(model) = config.get("model").and_then(|v| v.as_str()) {
                        println!("  Model:       {}", model);
                    }
                    if let Some(max_tokens) = config.get("max_tokens").and_then(|v| v.as_u64()) {
                        println!("  Max Tokens:  {}", max_tokens);
                    }
                    if let Some(temp) = config.get("temperature").and_then(|v| v.as_f64()) {
                        println!("  Temperature: {:.1}", temp);
                    }
                    if let Some(data_dir) = config.get("data_dir").and_then(|v| v.as_str()) {
                        println!("  Data Dir:    {}", data_dir);
                    }
                    if let Some(mem_dir) = config.get("memory_dir").and_then(|v| v.as_str()) {
                        println!("  Memory Dir:  {}", mem_dir);
                    }
                    if let Some(skills_dir) = config.get("skills_dir").and_then(|v| v.as_str()) {
                        println!("  Skills Dir:  {}", skills_dir);
                    }
                }
                println!();
                println!(
                    "LSP Servers:   {}",
                    json.get("lsp_servers")
                        .map(|v| v.as_array().map_or(0, |a| a.len()))
                        .unwrap_or(0)
                );
                println!(
                    "Memory Entries: {}",
                    json.get("memory_entries")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0)
                );
                println!(
                    "Skills:        {}",
                    json.get("skills").and_then(|v| v.as_u64()).unwrap_or(0)
                );
            }
        }
        Command::Config {
            command: ConfigCommand::Show,
        } => {
            let display = runtime.config().redacted_for_display();
            if cli.format == "json" {
                println!("{}", serde_json::to_string_pretty(&display)?);
            } else {
                println!("RustyCode Config");
                println!("================");
                if let Some(model) = display.get("model").and_then(|v| v.as_str()) {
                    println!("  Model:        {}", model);
                }
                if let Some(temp) = display.get("temperature").and_then(|v| v.as_f64()) {
                    println!("  Temperature:  {:.1}", temp);
                }
                if let Some(max_tokens) = display.get("max_tokens").and_then(|v| v.as_u64()) {
                    println!("  Max Tokens:   {}", max_tokens);
                }
                if let Some(data_dir) = display.get("data_dir").and_then(|v| v.as_str()) {
                    println!("  Data Dir:     {}", data_dir);
                }
                if let Some(mem_dir) = display.get("memory_dir").and_then(|v| v.as_str()) {
                    println!("  Memory Dir:   {}", mem_dir);
                }
                if let Some(skills_dir) = display.get("skills_dir").and_then(|v| v.as_str()) {
                    println!("  Skills Dir:   {}", skills_dir);
                }
                if let Some(providers) = display.get("providers").and_then(|v| v.as_object()) {
                    let active: Vec<_> = providers.iter().filter(|(_, v)| !v.is_null()).collect();
                    println!(
                        "  Providers:    {}",
                        active
                            .iter()
                            .map(|(k, _)| k.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
                if let Some(routing) = display.get("model_routing") {
                    if routing
                        .get("enabled")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                    {
                        println!("  Model Routing: enabled");
                    }
                }
                if let Some(routing) = display.get("task_routing") {
                    let enabled = routing
                        .get("enabled")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let threshold = routing
                        .get("confidence_threshold")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    let questions = routing
                        .get("max_clarifying_questions")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let research = routing
                        .get("max_research_passes")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    println!(
                        "  Task Routing: {} (threshold {:.2}, questions {}, research {})",
                        if enabled { "enabled" } else { "disabled" },
                        threshold,
                        questions,
                        research
                    );
                }
                if let Some(features) = display.get("features") {
                    let fw = features
                        .get("file_watcher")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let gi = features
                        .get("git_integration")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    println!("  File Watcher: {}", if fw { "on" } else { "off" });
                    println!("  Git Integration: {}", if gi { "on" } else { "off" });
                }
            }
        }
        Command::Config {
            command: ConfigCommand::Get { key },
        } => {
            let config = runtime.config();
            let resolved_key = resolve_config_key(&key);
            let value = match resolved_key.as_str() {
                "model" => config.model.clone(),
                "provider" => {
                    let display = config.redacted_for_display();
                    if let Some(providers) = display.get("providers") {
                        let active: Vec<_> = providers
                            .as_object()
                            .map(|obj| {
                                obj.iter()
                                    .filter(|(_, v)| !v.is_null())
                                    .map(|(k, _)| k.as_str())
                                    .collect()
                            })
                            .unwrap_or_default();
                        active.join(", ")
                    } else {
                        "none".to_string()
                    }
                }
                "temperature" => config
                    .temperature
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
                "max_tokens" => config.max_tokens.map(|v| v.to_string()).unwrap_or_default(),
                "data_dir" => config.data_dir.display().to_string(),
                "memory_dir" => config.memory_dir.display().to_string(),
                "skills_dir" => config.skills_dir.display().to_string(),
                _ => {
                    let display = config.redacted_for_display();
                    if let Some(found) = get_value_at_path(&display, &resolved_key) {
                        config_value_to_string(found)
                    } else {
                        eprintln!(
                            "Unknown config key: {}. Run 'rustycode config show' to see all keys.",
                            key
                        );
                        std::process::exit(1);
                    }
                }
            };
            if cli.format == "json" {
                println!("{}", serde_json::json!(value));
            } else {
                println!("{}", value);
            }
        }
        Command::Config {
            command: ConfigCommand::Set { key, value },
        } => {
            let resolved_key = resolve_config_key(&key);

            let config_path = RustyCodePath::global_config_file()
                .unwrap_or_else(|_| PathBuf::from(".rustycode/config.json"));

            if !config_path.exists() {
                eprintln!("Config file not found at {}", config_path.display());
                eprintln!("Run 'rustycode config show' to see current configuration.");
                std::process::exit(1);
            }

            let content = std::fs::read_to_string(&config_path)
                .with_context(|| format!("Failed to read {}", config_path.display()))?;
            let mut config_json: serde_json::Value = serde_json::from_str(&content)
                .with_context(|| format!("Failed to parse {}", config_path.display()))?;

            // Parse value with proper type inference: bool > i64 > f64 > string
            let json_value = if let Ok(b) = value.parse::<bool>() {
                serde_json::json!(b)
            } else if let Ok(n) = value.parse::<i64>() {
                serde_json::json!(n)
            } else if let Ok(n) = value.parse::<f64>() {
                serde_json::json!(n)
            } else {
                serde_json::json!(value)
            };

            set_value_at_path(&mut config_json, &resolved_key, json_value)
                .with_context(|| format!("Failed to update config path '{}'", resolved_key))?;

            let output = serde_json::to_string_pretty(&config_json)?;
            std::fs::write(&config_path, output)
                .with_context(|| format!("Failed to write {}", config_path.display()))?;

            if cli.format == "json" {
                println!(
                    "{}",
                    serde_json::json!({"key": key, "value": value, "status": "updated"})
                );
            } else {
                println!("Set {} = {}", key, value);
                println!("Config updated at {}", config_path.display());
            }
        }
        Command::Context { prompt } => {
            let report = runtime.run(&cwd, &prompt).await?;
            println!("{}", serde_json::to_string_pretty(&report.context_plan)?);
        }
        Command::Run {
            prompt,
            auto: _,
            format,
            mode: _,
            use_ast: _,
            ast_complexity: _,
            json_schema,
        } => {
            // Unify execution through OrchestrationPipeline
            use rustycode_llm::{create_provider_with_config, load_provider_config_from_env};
            use rustycode_orchestration::config::OrchestrationConfig;
            use rustycode_orchestration::pipeline::OrchestrationPipeline;

            let (provider_type, model_name, v2_config) =
                load_provider_config_from_env().context("Failed to load LLM provider config")?;
            let provider = create_provider_with_config(&provider_type, &model_name, v2_config)
                .context("Failed to create LLM provider")?;

            let config = OrchestrationConfig::default();
            let mut pipeline =
                OrchestrationPipeline::with_provider_and_model(config, provider, &model_name);

            // Parse JSON schema if provided
            if let Some(schema_input) = json_schema {
                let schema_str = if let Some(stripped) = schema_input.strip_prefix('@') {
                    std::fs::read_to_string(stripped)
                        .with_context(|| format!("Failed to read schema file: {stripped}"))?
                } else {
                    schema_input
                };
                let schema: serde_json::Value = serde_json::from_str(&schema_str)
                    .with_context(|| "Failed to parse JSON schema")?;
                pipeline = pipeline.with_output_schema(schema);
            }

            let session_id = SessionId::new();

            // Execute via unified pipeline
            let result = pipeline
                .conduct(session_id.to_string(), prompt.clone())
                .await?;

            match result {
                rustycode_orchestration::pipeline::TaskResult::Success {
                    output,
                    total_cost,
                    tier_used,
                    structured_output,
                    ..
                } => {
                    if let Some(so) = &structured_output {
                        // When structured output was requested, print it as JSON to stdout
                        println!("{}", serde_json::to_string_pretty(so)?);
                    } else if format == "json" {
                        println!(
                            "{}",
                            serde_json::json!({
                                "status": "success",
                                "session_id": session_id.to_string(),
                                "output": output,
                                "cost": total_cost,
                                "tier": tier_used
                            })
                        );
                    } else {
                        println!("\n✅ Task completed successfully");
                        println!("\n{}", output);
                    }

                    // Save conversation history (best-effort)
                    if let Err(e) = (|| -> Result<()> {
                        use rustycode_storage::conversation_history::{
                            now_timestamp, Conversation, ConversationHistory, SavedMessage,
                        };

                        let created_at = now_timestamp();
                        let title = if prompt.len() > 80 {
                            format!("{}...", &prompt[..prompt.floor_char_boundary(80)])
                        } else {
                            prompt.clone()
                        };
                        let cost_cents = (total_cost * 100.0).round() as u32;

                        let conversation = Conversation {
                            id: session_id.to_string(),
                            title,
                            model: model_name.clone(),
                            provider: provider_type.clone(),
                            created_at,
                            updated_at: now_timestamp(),
                            messages: vec![
                                SavedMessage {
                                    role: "user".into(),
                                    content: prompt.clone(),
                                    timestamp: created_at,
                                    tokens: None,
                                    model: None,
                                    provider: None,
                                },
                                SavedMessage {
                                    role: "assistant".into(),
                                    content: output.clone(),
                                    timestamp: now_timestamp(),
                                    tokens: None,
                                    model: Some(model_name.clone()),
                                    provider: Some(provider_type.clone()),
                                },
                            ],
                            tags: vec![],
                            total_tokens: 0,
                            total_cost_cents: cost_cents,
                            workspace_path: Some(
                                std::env::current_dir()?.to_string_lossy().into_owned(),
                            ),
                        };

                        let history = ConversationHistory::default_dir()?;
                        history.save(&conversation)?;
                        Ok(())
                    })() {
                        eprintln!("Warning: failed to save history: {e}");
                    }
                }
                rustycode_orchestration::pipeline::TaskResult::Failed {
                    reason,
                    total_cost,
                    ..
                } => {
                    if format == "json" {
                        eprintln!(
                            "{}",
                            serde_json::json!({
                                "status": "failed",
                                "reason": reason,
                                "cost": total_cost
                            })
                        );
                    } else {
                        eprintln!("\n❌ Task failed: {}", reason);
                    }
                    std::process::exit(1);
                }
            }
        }
        Command::Tools {
            command: ToolsCommand::List,
        } => {
            let tools = runtime.tool_list();
            if cli.format == "json" {
                println!("{}", serde_json::to_string_pretty(&tools)?);
            } else {
                let width = tools.iter().map(|tool| tool.name.len()).max().unwrap_or(0);
                for tool in &tools {
                    println!("{:<width$}  {}", tool.name, tool.description, width = width);
                }
            }
        }
        Command::Tools {
            command: ToolsCommand::Call { name, params },
        } => {
            let arguments: serde_json::Value = serde_json::from_str(&params)
                .map_err(|e| anyhow::anyhow!("--params must be valid JSON: {e}"))?;
            let report = runtime.run_tool(&cwd, name, arguments).await?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Command::Sessions {
            command: SessionsCommand::List { limit },
        } => {
            let sessions = runtime.recent_sessions(limit).await?;
            if cli.format == "json" {
                println!("{}", serde_json::to_string_pretty(&sessions)?);
            } else {
                println!("Recent Sessions");
                println!("===============");
                for s in &sessions {
                    println!(
                        "  {}  {}  {:?}  {}",
                        s.id,
                        s.created_at.format("%Y-%m-%d %H:%M"),
                        s.mode,
                        s.task
                    );
                }
                if sessions.is_empty() {
                    println!("  No sessions found.");
                }
            }
        }
        Command::Sessions {
            command: SessionsCommand::Show { id },
        } => {
            let session_id = SessionId::parse(&id)?;
            let sessions = runtime.recent_sessions(1000).await?;
            let found = sessions.iter().find(|s| s.id == session_id);
            if found.is_none() {
                eprintln!("Session not found: {}", id);
                std::process::exit(1);
            }
            let session = found.expect("checked is_none above with exit");
            let events = runtime.session_events(&session_id).await?;
            if cli.format == "json" {
                let output = serde_json::json!({
                    "session": session,
                    "events": events,
                });
                println!("{}", serde_json::to_string_pretty(&output)?);
            } else {
                println!("Session: {}", session.id);
                println!("  Task:    {}", session.task);
                println!(
                    "  Created: {}",
                    session.created_at.format("%Y-%m-%d %H:%M:%S")
                );
                println!("  Mode:    {:?}", session.mode);
                println!("  Status:  {}", session.status);
                println!();
                if events.is_empty() {
                    println!("  No events.");
                } else {
                    println!("Events ({}):", events.len());
                    for evt in &events {
                        println!("  {:?}", evt);
                    }
                }
            }
        }
        Command::Events {
            command:
                EventsCommand::Watch {
                    pattern,
                    limit,
                    timeout_ms,
                    run,
                    tool,
                    plan,
                    approve_session,
                    reject_session,
                    params,
                },
        } => {
            watch_events(
                &runtime,
                &pattern,
                limit,
                timeout_ms,
                run,
                tool,
                plan,
                approve_session,
                reject_session,
                &params,
            )
            .await?;
        }
        Command::Plan { command } => {
            commands::plan_cmd::execute(&runtime, &cwd, command, &cli.format).await?
        }
        Command::Agent { command } => commands::agent_cmd::execute(&runtime, &cwd, command).await?,
        Command::Harness { command } => harness_cmd::execute(&cwd, command).await?,
        Command::Omo { command } => commands::omo_cmd::execute(command).await?,
        Command::Worktree { command } => {
            commands::worktree_cmd::execute(&cwd, command, &cli.format).await?
        }
        Command::Provider { command } => {
            provider_cmd::execute(command, &cli.format).await?;
        }
        Command::History { command } => {
            history_cmd::execute(command)?;
        }
        Command::Skills { command } => {
            skills_cmd::execute(command, &cli.format).await?;
        }
        Command::Learnings { command } => {
            execute_learnings_command(&cwd, command, &cli.format)?;
        }
        Command::Bench { command } => {
            commands::bench_cmd::execute(command).await?;
        }
        Command::Ast { command } => commands::ast_cmd::execute(&cwd, command).await?,
        #[allow(unreachable_patterns)]
        _ => {
            anyhow::bail!("Unknown command. Run 'rustycode --help' for available commands.");
        }
    }
    Ok(())
}

fn execute_learnings_command(
    cwd: &std::path::Path,
    command: LearningsCommand,
    format: &str,
) -> Result<()> {
    use rustycode_core::team::team_learnings::{LearningCategory, TeamLearnings};

    let mut learnings = TeamLearnings::load(cwd)?;

    match command {
        LearningsCommand::Show | LearningsCommand::List => {
            if format == "json" {
                println!(
                    "{}",
                    serde_json::json!({
                        "learnings": learnings.all(),
                    })
                );
            } else {
                println!("{}", learnings.all());
            }
        }
        LearningsCommand::Add { category, content } => {
            let cat = match category.as_str() {
                "user-preference" => LearningCategory::UserPreference,
                "codebase-quirk" => LearningCategory::CodebaseQuirk,
                "what-worked" => LearningCategory::WhatWorked,
                "what-failed" => LearningCategory::WhatFailed,
                _ => {
                    eprintln!("Invalid category: {}. Use: user-preference, codebase-quirk, what-worked, what-failed", category);
                    std::process::exit(1);
                }
            };
            learnings.record(cat, content, None);
            learnings.save()?;
            println!("✓ Learning recorded");
        }
        LearningsCommand::Remove { category, content } => {
            let cat = match category.as_str() {
                "user-preference" => LearningCategory::UserPreference,
                "codebase-quirk" => LearningCategory::CodebaseQuirk,
                "what-worked" => LearningCategory::WhatWorked,
                "what-failed" => LearningCategory::WhatFailed,
                _ => {
                    eprintln!("Invalid category: {}. Use: user-preference, codebase-quirk, what-worked, what-failed", category);
                    std::process::exit(1);
                }
            };
            if learnings.remove(&cat, &content) {
                println!("✓ Learning removed");
            } else {
                eprintln!("No matching learning found");
            }
        }
        LearningsCommand::Clear { yes } => {
            if yes {
                learnings.clear();
                learnings.save()?;
                println!("✓ All learnings cleared");
            } else {
                eprintln!("⚠️  This will delete all learnings. Re-run with --yes to confirm.");
            }
        }
        #[allow(unreachable_patterns)]
        _ => {
            eprintln!("Unknown learnings command. Run --help for usage.");
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn watch_events(
    runtime: &AsyncRuntime,
    pattern: &str,
    limit: usize,
    timeout_ms: u64,
    run: Option<String>,
    tool: Option<String>,
    plan: Option<String>,
    approve_session: Option<String>,
    reject_session: Option<String>,
    params: &str,
) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let (_subscription_id, mut rx) = runtime.subscribe_events(pattern).await?;

    if let Some(prompt) = run {
        runtime.run(&cwd, &prompt).await?;
    }

    if let Some(tool_name) = tool {
        let arguments: serde_json::Value = serde_json::from_str(params)
            .map_err(|e| anyhow::anyhow!("--params must be valid JSON: {e}"))?;
        runtime.run_tool(&cwd, tool_name, arguments).await?;
    }

    if let Some(task) = plan {
        runtime.start_planning(&cwd, &task).await?;
    }

    if let Some(session_id) = approve_session {
        let session_id = SessionId::parse(&session_id)?;
        runtime.approve_plan(&session_id, &cwd).await?;
    }

    if let Some(session_id) = reject_session {
        let session_id = SessionId::parse(&session_id)?;
        runtime.reject_plan(&session_id, &cwd).await?;
    }

    for _ in 0..limit {
        let event = tokio::time::timeout(Duration::from_millis(timeout_ms), rx.recv()).await;
        match event {
            Ok(Ok(event)) => {
                let payload = rustycode_bus::Event::serialize(event.as_ref());
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "event_type": event.event_type(),
                        "payload": payload,
                    }))?
                );
            }
            Ok(Err(error)) => {
                return Err(anyhow::anyhow!("event subscription closed: {error}"));
            }
            Err(_) => break,
        }
    }
    Ok(())
}

/// Suggest similar subcommand names when a task looks like it could be a subcommand
fn suggest_similar_subcommand(task: &str) -> Option<String> {
    const SUBCOMMANDS: &[&str] = &[
        "doctor",
        "config",
        "context",
        "run",
        "tools",
        "sessions",
        "events",
        "plan",
        "agent",
        "harness",
        "omo",
        "worktree",
        "provider",
        "history",
        "skills",
        "learnings",
        "tui",
        "serve",
        "swebench",
        "bench",
        "ast",
    ];

    // Check if task looks like a subcommand (lowercase, no spaces, no special chars)
    let looks_like_command = task
        .chars()
        .all(|c| c.is_lowercase() || c == '-' || c == '_')
        && !task.contains(' ')
        && task.len() >= 2
        && task.len() <= 20;

    if !looks_like_command {
        return None;
    }

    // Find closest match using simple Levenshtein distance
    let task_lower = task.to_lowercase();
    let mut best_match: Option<(&str, usize)> = None;

    for &subcmd in SUBCOMMANDS {
        let distance = levenshtein_distance(&task_lower, subcmd);
        // Only suggest if within 3 edits and not exact match
        if distance <= 3 && distance > 0 {
            match best_match {
                None => best_match = Some((subcmd, distance)),
                Some((_, d)) if distance < d => best_match = Some((subcmd, distance)),
                _ => {}
            }
        }
    }

    best_match.map(|(subcmd, _)| subcmd.to_string())
}

/// Simple Levenshtein distance implementation
#[allow(clippy::needless_range_loop)]
fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let a_len = a_chars.len();
    let b_len = b_chars.len();

    if a_len == 0 {
        return b_len;
    } else if b_len == 0 {
        return a_len;
    }

    let mut matrix = vec![vec![0usize; b_len + 1]; a_len + 1];

    for i in 0..=a_len {
        matrix[i][0] = i;
    }
    for j in 0..=b_len {
        matrix[0][j] = j;
    }

    for i in 1..=a_len {
        for j in 1..=b_len {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            matrix[i][j] = std::cmp::min(
                std::cmp::min(matrix[i - 1][j] + 1, matrix[i][j - 1] + 1),
                matrix[i - 1][j - 1] + cost,
            );
        }
    }

    matrix[a_len][b_len]
}
