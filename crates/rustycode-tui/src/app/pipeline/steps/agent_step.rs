use crate::app::pipeline::agent_manager::{TuiAgentBridge, TuiAgentManager};
use crate::app::pipeline::manifest::StepDefinition;
use crate::app::pipeline::registry::{Dependency, PipelineContext, PipelineStep, Signal};
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_yaml::Value;

pub struct AgentStep {
    id: String,
    name: String,
    task: String,
    model: Option<String>,
    dependencies: Vec<Dependency>,
    provides: Vec<Signal>,
}

impl AgentStep {
    pub fn from_definition(def: &StepDefinition) -> Self {
        let params = def.params.as_ref();
        let name =
            param_string(params, "name").unwrap_or_else(|| format!("Agent Step ({})", def.id));
        let task = param_string(params, "task")
            .unwrap_or_else(|| format!("Execute the generic pipeline step '{}'.", def.id));
        let model = param_string(params, "model");
        let provides = param_strings(params, "provides")
            .into_iter()
            .map(Signal)
            .collect();

        Self {
            id: def.id.clone(),
            name,
            task,
            model,
            dependencies: Vec::new(),
            provides,
        }
    }
}

#[async_trait]
impl PipelineStep for AgentStep {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn dependencies(&self) -> Vec<Dependency> {
        self.dependencies.clone()
    }

    fn provides(&self) -> Vec<Signal> {
        self.provides.clone()
    }

    async fn execute(&self, ctx: &mut PipelineContext) -> Result<()> {
        let agent_manager = TuiAgentManager::new(
            std::env::current_dir().context("failed to resolve current directory")?,
            ctx.provider.clone(),
            ctx.agent_config.clone(),
        );

        let (tx, _rx) = std::sync::mpsc::sync_channel(100);
        let mut events = TuiAgentBridge::new(tx);
        let _result = agent_manager
            .run_task(
                self.model.as_deref().unwrap_or(&ctx.current_model),
                &self.task,
                &ctx.agent_tool_registry,
                &mut events,
            )
            .await
            .with_context(|| format!("agent step '{}' failed", self.id))?;

        Ok(())
    }
}

fn param_string(
    params: Option<&std::collections::HashMap<String, Value>>,
    key: &str,
) -> Option<String> {
    params
        .and_then(|params| params.get(key))
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn param_strings(
    params: Option<&std::collections::HashMap<String, Value>>,
    key: &str,
) -> Vec<String> {
    params
        .and_then(|params| params.get(key))
        .and_then(Value::as_sequence)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}
