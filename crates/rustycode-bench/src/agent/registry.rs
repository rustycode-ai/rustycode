//! Agent registry — extensible agent factory.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{bail, Result};

use crate::agent::{BenchAgent, CodeAgent, CodeAgentConfig, NopAgent, OracleAgent};

/// Factory function type for creating agents.
pub type AgentFactory = fn(&str, &str, PathBuf) -> Result<Box<dyn BenchAgent>>;

/// Registry of named agent factories.
#[derive(Default)]
pub struct AgentRegistry {
    factories: HashMap<String, AgentFactory>,
}

impl AgentRegistry {
    /// Create a registry with the built-in agents (oracle, code, nop).
    pub fn new() -> Self {
        let mut factories: HashMap<String, AgentFactory> = HashMap::new();
        factories.insert("oracle".to_string(), oracle_factory);
        factories.insert("code".to_string(), code_factory);
        factories.insert("nop".to_string(), nop_factory);
        #[cfg(feature = "real-agent")]
        factories.insert("real".to_string(), real_factory);
        Self { factories }
    }

    /// Register a custom agent factory.
    pub fn register(&mut self, name: impl Into<String>, factory: AgentFactory) {
        self.factories.insert(name.into(), factory);
    }

    /// Create an agent by name.
    pub fn create(
        &self,
        name: &str,
        model: &str,
        solution_dir: PathBuf,
    ) -> Result<Box<dyn BenchAgent>> {
        match self.factories.get(name) {
            Some(factory) => factory(name, model, solution_dir),
            None => {
                let available: Vec<&str> = self.factories.keys().map(String::as_str).collect();
                bail!(
                    "Unknown agent: '{name}'. Available: {}",
                    available.join(", ")
                )
            }
        }
    }

    /// List registered agent names.
    pub fn list(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.factories.keys().map(String::as_str).collect();
        names.sort();
        names
    }
}

#[allow(clippy::unnecessary_wraps)]
fn oracle_factory(_name: &str, _model: &str, solution_dir: PathBuf) -> Result<Box<dyn BenchAgent>> {
    Ok(Box::new(OracleAgent::new(solution_dir)) as Box<dyn BenchAgent>)
}

fn code_factory(_name: &str, model: &str, _solution_dir: PathBuf) -> Result<Box<dyn BenchAgent>> {
    let (provider, model_name) = crate::config::resolve_provider_model(model)?;
    let cfg = CodeAgentConfig {
        provider,
        model: model_name,
        ..Default::default()
    };
    let agent = CodeAgent::auto(cfg)?;
    Ok(Box::new(agent) as Box<dyn BenchAgent>)
}

#[allow(clippy::unnecessary_wraps)]
fn nop_factory(_name: &str, _model: &str, _solution_dir: PathBuf) -> Result<Box<dyn BenchAgent>> {
    Ok(Box::new(NopAgent) as Box<dyn BenchAgent>)
}

#[cfg(feature = "real-agent")]
fn real_factory(_name: &str, model: &str, _solution_dir: PathBuf) -> Result<Box<dyn BenchAgent>> {
    super::real_agent::real_agent_factory(_name, model, _solution_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_builtin_agents() {
        let registry = AgentRegistry::new();
        let names = registry.list();
        assert!(names.contains(&"code"));
        assert!(names.contains(&"nop"));
        assert!(names.contains(&"oracle"));
        #[cfg(feature = "real-agent")]
        assert!(names.contains(&"real"));
    }

    #[test]
    fn registry_creates_oracle() {
        let tmp = std::env::temp_dir().join("rtk-bench-registry-oracle");
        let _ = std::fs::create_dir_all(&tmp);
        let registry = AgentRegistry::new();
        let agent = registry.create("oracle", "auto", tmp.clone()).unwrap();
        assert_eq!(agent.name(), "oracle");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn registry_creates_nop() {
        let registry = AgentRegistry::new();
        let agent = registry
            .create("nop", "auto", PathBuf::from("/tmp"))
            .unwrap();
        assert_eq!(agent.name(), "nop");
    }

    #[test]
    fn registry_unknown_agent_returns_error() {
        let registry = AgentRegistry::new();
        let result = registry.create("nonexistent", "auto", PathBuf::from("/tmp"));
        assert!(result.is_err());
        assert!(result.err().unwrap().to_string().contains("Unknown agent"));
    }

    #[test]
    fn registry_register_custom() {
        #[allow(clippy::unnecessary_wraps)]
        fn custom_factory(_name: &str, _model: &str, _sol: PathBuf) -> Result<Box<dyn BenchAgent>> {
            Ok(Box::new(NopAgent) as Box<dyn BenchAgent>)
        }

        let mut registry = AgentRegistry::new();
        registry.register("custom", custom_factory);
        let names = registry.list();
        assert!(names.contains(&"custom"));
        let agent = registry
            .create("custom", "auto", PathBuf::from("/tmp"))
            .unwrap();
        assert_eq!(agent.name(), "nop");
    }
}
