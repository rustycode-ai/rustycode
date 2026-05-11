//! Agent factory, provider resolution, and benchmark configuration.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::agent::{BenchAgent, CodeAgent, CodeAgentConfig, NopAgent, OracleAgent};
use crate::trial::RetryConfig;

// Agent factory

/// Create the appropriate agent based on the name.
pub fn create_agent(
    agent_name: &str,
    model: &str,
    solution_dir: PathBuf,
    provider_override: Option<&str>,
) -> Result<Box<dyn BenchAgent>> {
    match agent_name {
        "oracle" => Ok(Box::new(OracleAgent::new(solution_dir)) as Box<dyn BenchAgent>),
        "nop" => Ok(Box::new(NopAgent) as Box<dyn BenchAgent>),
        "code" => {
            let (auto_provider, model_name) = resolve_provider_model(model)?;
            let provider = provider_override
                .map(|s| s.to_string())
                .unwrap_or(auto_provider);
            let cfg = CodeAgentConfig {
                provider,
                model: model_name,
                ..Default::default()
            };
            let agent = CodeAgent::auto(cfg).context("Failed to create CodeAgent")?;
            Ok(Box::new(agent) as Box<dyn BenchAgent>)
        }
        #[cfg(feature = "real-agent")]
        "real" => {
            let (_provider, model_name) = resolve_provider_model(model)?;
            let agent =
                crate::agent::real_agent::real_agent_factory("real", &model_name, solution_dir)?;
            Ok(agent)
        }
        other => bail!("Unknown agent: '{other}'. Available: oracle, code, nop"),
    }
}

/// Resolve provider and model from the --model flag or environment.
///
/// If model is "auto", detects from available API keys.
pub fn resolve_provider_model(model: &str) -> Result<(String, String)> {
    if model != "auto" {
        if model.starts_with("claude") {
            return Ok(("anthropic".to_string(), model.to_string()));
        }
        if model.starts_with("gpt") || model.starts_with("o1") || model.starts_with("o3") {
            return Ok(("openai".to_string(), model.to_string()));
        }
        if model.starts_with("gemini") {
            return Ok(("gemini".to_string(), model.to_string()));
        }
        if model.starts_with("glm") {
            return Ok(("zhipu".to_string(), model.to_string()));
        }
        return Ok(("anthropic".to_string(), model.to_string()));
    }

    if std::env::var("ANTHROPIC_API_KEY")
        .or_else(|_| std::env::var("ANTHROPIC_AUTH_TOKEN"))
        .is_ok()
    {
        Ok(("anthropic".to_string(), "claude-sonnet-4-6".to_string()))
    } else if std::env::var("OPENAI_API_KEY").is_ok() {
        Ok(("openai".to_string(), "gpt-4o".to_string()))
    } else if std::env::var("GOOGLE_API_KEY").is_ok() {
        Ok(("gemini".to_string(), "gemini-2.5-pro".to_string()))
    } else {
        bail!("No API key found. Set ANTHROPIC_API_KEY (or ANTHROPIC_AUTH_TOKEN), OPENAI_API_KEY, or GOOGLE_API_KEY")
    }
}

// Benchmark configuration (Harbor-compatible config file support)

/// Top-level benchmark configuration, loadable from JSON or TOML.
///
/// Harbor equivalent: the JSON config files in `harbor-agent/configs/*.json`.
///
/// # Merge order
///
/// Defaults < config file < CLI flags
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchConfig {
    /// Dataset reference: local path or registry ref (e.g. "terminal-bench@2.0")
    #[serde(default = "default_dataset")]
    pub dataset: String,

    /// Agent configuration.
    #[serde(default)]
    pub agent: AgentConfig,

    /// Execution environment: "native" or "docker".
    #[serde(default = "default_env")]
    pub env: String,

    /// Number of concurrent trials.
    #[serde(default = "default_concurrent")]
    pub n_concurrent: usize,

    /// Retry behavior for infrastructure failures.
    #[serde(default)]
    pub retry: RetryConfig,

    /// Per-trial wall-clock timeout in seconds (0 = auto).
    #[serde(default)]
    pub timeout: u64,

    /// Comma-separated task name filter (substring match).
    #[serde(default)]
    pub task_filter: Option<String>,

    /// Maximum number of tasks to run.
    #[serde(default)]
    pub max_tasks: Option<usize>,

    /// Human-readable job name.
    #[serde(default)]
    pub job_name: Option<String>,

    /// Force rebuild container images.
    #[serde(default)]
    pub force_build: bool,

    /// Override provider auto-detection: "anthropic" or "openai".
    #[serde(default)]
    pub provider: Option<String>,

    /// Cleanup containers after trials.
    #[serde(default = "default_true")]
    pub cleanup: bool,

    /// Output format: "pretty", "json", "csv", "markdown", "summary".
    #[serde(default = "default_output")]
    pub output: String,

    /// Environment resource overrides (Docker only).
    #[serde(default)]
    pub resources: Option<ResourceOverrides>,
}

fn default_dataset() -> String {
    ".".to_string()
}

fn default_env() -> String {
    "native".to_string()
}

fn default_concurrent() -> usize {
    1
}

fn default_true() -> bool {
    true
}

fn default_output() -> String {
    "pretty".to_string()
}

impl Default for BenchConfig {
    fn default() -> Self {
        Self {
            dataset: default_dataset(),
            agent: AgentConfig::default(),
            env: default_env(),
            n_concurrent: default_concurrent(),
            retry: RetryConfig::default(),
            timeout: 0,
            task_filter: None,
            max_tasks: None,
            job_name: None,
            force_build: false,
            cleanup: true,
            output: default_output(),
            resources: None,
            provider: None,
        }
    }
}

impl BenchConfig {
    /// Load configuration from a JSON or TOML file.
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config from {}", path.display()))?;

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

        match ext {
            "toml" => toml::from_str(&content)
                .with_context(|| format!("Failed to parse TOML config {}", path.display())),
            _ => serde_json::from_str(&content)
                .with_context(|| format!("Failed to parse JSON config {}", path.display())),
        }
    }

    /// Merge CLI overrides into this config.
    ///
    /// Only non-default CLI values override the file config.
    /// This is a simple layer: for each field, if the override is `Some`, use it.
    pub fn merge_cli(
        self,
        dataset: Option<String>,
        agent_name: Option<String>,
        model: Option<String>,
        env: Option<String>,
        n_concurrent: Option<usize>,
        timeout: Option<u64>,
        task_filter: Option<String>,
        max_tasks: Option<usize>,
        job_name: Option<String>,
        force_build: Option<bool>,
        cleanup: Option<bool>,
        retry: Option<usize>,
        output: Option<String>,
    ) -> Self {
        Self {
            dataset: dataset.unwrap_or(self.dataset),
            agent: AgentConfig {
                name: agent_name.unwrap_or(self.agent.name),
                model: model.unwrap_or(self.agent.model),
                system_prompt: self.agent.system_prompt,
                max_turns: self.agent.max_turns,
            },
            env: env.unwrap_or(self.env),
            n_concurrent: n_concurrent.unwrap_or(self.n_concurrent),
            timeout: timeout.unwrap_or(self.timeout),
            task_filter: task_filter.or(self.task_filter),
            max_tasks: max_tasks.or(self.max_tasks),
            job_name: job_name.or(self.job_name),
            force_build: force_build.unwrap_or(self.force_build),
            cleanup: cleanup.unwrap_or(self.cleanup),
            retry: retry
                .map(|r| RetryConfig {
                    max_retries: r,
                    ..self.retry.clone()
                })
                .unwrap_or_else(|| self.retry.clone()),
            output: output.unwrap_or(self.output),
            resources: self.resources,
            provider: self.provider,
        }
    }
}

/// Agent configuration within a benchmark config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Agent name: "oracle", "code", "nop".
    #[serde(default = "default_agent_name")]
    pub name: String,

    /// Model for the code agent.
    #[serde(default = "default_model")]
    pub model: String,

    /// Optional system prompt override.
    #[serde(default)]
    pub system_prompt: Option<String>,

    /// Maximum tool turns for the code agent.
    #[serde(default)]
    pub max_turns: Option<usize>,
}

fn default_agent_name() -> String {
    "oracle".to_string()
}

fn default_model() -> String {
    "auto".to_string()
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            name: default_agent_name(),
            model: default_model(),
            system_prompt: None,
            max_turns: None,
        }
    }
}

/// Docker resource overrides (maps to Harbor's `override_resources`).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResourceOverrides {
    pub memory: Option<String>,
    pub cpus: Option<f64>,
    pub network_mode: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_provider_model_claude() {
        let (provider, model) = resolve_provider_model("claude-sonnet-4-6").unwrap();
        assert_eq!(provider, "anthropic");
        assert_eq!(model, "claude-sonnet-4-6");
    }

    #[test]
    fn resolve_provider_model_gpt() {
        let (provider, model) = resolve_provider_model("gpt-4o").unwrap();
        assert_eq!(provider, "openai");
        assert_eq!(model, "gpt-4o");
    }

    #[test]
    fn resolve_provider_model_gemini() {
        let (provider, model) = resolve_provider_model("gemini-2.5-pro").unwrap();
        assert_eq!(provider, "gemini");
        assert_eq!(model, "gemini-2.5-pro");
    }

    #[test]
    fn resolve_provider_model_fallback() {
        let (provider, model) = resolve_provider_model("some-unknown-model").unwrap();
        assert_eq!(provider, "anthropic");
        assert_eq!(model, "some-unknown-model");
    }

    #[test]
    fn create_agent_oracle() {
        let tmp = std::env::temp_dir().join("rtk-bench-test-oracle");
        let _ = std::fs::create_dir_all(&tmp);
        let agent = create_agent("oracle", "auto", tmp.clone(), None);
        assert!(agent.is_ok());
        assert_eq!(agent.unwrap().name(), "oracle");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn create_agent_nop() {
        let agent = create_agent("nop", "auto", PathBuf::from("/tmp"), None);
        assert!(agent.is_ok());
        assert_eq!(agent.unwrap().name(), "nop");
    }

    #[test]
    fn create_agent_unknown() {
        let agent = create_agent("nonexistent", "auto", PathBuf::from("/tmp"), None);
        assert!(agent.is_err());
        assert!(agent.err().unwrap().to_string().contains("Unknown agent"));
    }

    #[test]
    fn bench_config_default() {
        let config = BenchConfig::default();
        assert_eq!(config.dataset, ".");
        assert_eq!(config.agent.name, "oracle");
        assert_eq!(config.env, "native");
        assert_eq!(config.n_concurrent, 1);
        assert!(config.cleanup);
        assert_eq!(config.output, "pretty");
    }

    #[test]
    fn bench_config_json_roundtrip() {
        let config = BenchConfig::default();
        let json = serde_json::to_string_pretty(&config).unwrap();
        let parsed: BenchConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.dataset, config.dataset);
        assert_eq!(parsed.agent.name, config.agent.name);
        assert_eq!(parsed.n_concurrent, config.n_concurrent);
    }

    #[test]
    fn bench_config_toml_roundtrip() {
        let config = BenchConfig::default();
        let toml_str = toml::to_string(&config).unwrap();
        let parsed: BenchConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.dataset, config.dataset);
        assert_eq!(parsed.agent.name, config.agent.name);
    }

    #[test]
    fn bench_config_merge_cli_overrides() {
        let base = BenchConfig::default();
        let merged = base.merge_cli(
            Some("my-dataset".to_string()), // dataset
            Some("code".to_string()),       // agent_name
            None,                           // model (keep default)
            None,                           // env
            Some(4),                        // n_concurrent
            None,                           // timeout
            Some("regex".to_string()),      // task_filter
            None,                           // max_tasks
            None,                           // job_name
            None,                           // force_build
            None,                           // cleanup
            None,                           // retry
            None,                           // output
        );
        assert_eq!(merged.dataset, "my-dataset");
        assert_eq!(merged.agent.name, "code");
        assert_eq!(merged.agent.model, "auto"); // kept default
        assert_eq!(merged.n_concurrent, 4);
        assert_eq!(merged.task_filter.as_deref(), Some("regex"));
    }

    #[test]
    fn bench_config_load_json_file() {
        let dir = std::env::temp_dir().join("rtk-bench-config-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test-config.json");

        let json = r#"{"dataset": "tb2", "agent": {"name": "oracle"}, "n_concurrent": 2}"#;
        std::fs::write(&path, json).unwrap();

        let config = BenchConfig::load(&path).unwrap();
        assert_eq!(config.dataset, "tb2");
        assert_eq!(config.agent.name, "oracle");
        assert_eq!(config.n_concurrent, 2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bench_config_load_toml_file() {
        let dir = std::env::temp_dir().join("rtk-bench-config-test-toml");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test-config.toml");

        let toml_str = r#"
dataset = "tb2"
[agent]
name = "code"
model = "claude-sonnet-4-6"
n_concurrent = 8
"#;
        std::fs::write(&path, toml_str).unwrap();

        let config = BenchConfig::load(&path).unwrap();
        assert_eq!(config.dataset, "tb2");
        assert_eq!(config.agent.name, "code");
        assert_eq!(config.agent.model, "claude-sonnet-4-6");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resource_overrides_all_fields() {
        let json = r#"{"memory": "4g", "cpus": 2.0, "network_mode": "host"}"#;
        let ro: ResourceOverrides = serde_json::from_str(json).unwrap();
        assert_eq!(ro.memory.as_deref(), Some("4g"));
        assert_eq!(ro.cpus, Some(2.0));
        assert_eq!(ro.network_mode.as_deref(), Some("host"));
    }

    #[test]
    fn resource_overrides_empty() {
        let json = r#"{}"#;
        let ro: ResourceOverrides = serde_json::from_str(json).unwrap();
        assert!(ro.memory.is_none());
        assert!(ro.cpus.is_none());
        assert!(ro.network_mode.is_none());
    }
}
