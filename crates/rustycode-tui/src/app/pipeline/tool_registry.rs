use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;

pub trait Tool: Send + Sync {
    fn execute(&self, args: serde_json::Value) -> Result<serde_json::Value>;
}

#[derive(Default, Clone)]
#[non_exhaustive]
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
    namespaces: HashMap<String, Vec<String>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            namespaces: HashMap::new(),
        }
    }

    pub fn register(&mut self, namespace: &str, name: &str, tool: Arc<dyn Tool>) {
        let full_name = format!("{}.{}", namespace, name);
        self.tools.insert(full_name.clone(), tool);
        self.namespaces
            .entry(namespace.to_string())
            .or_default()
            .push(full_name);
    }

    pub fn get_tools_for_namespaces(
        &self,
        namespaces: &[String],
    ) -> HashMap<String, Arc<dyn Tool>> {
        let mut allowed = HashMap::new();
        for ns in namespaces {
            if let Some(tool_names) = self.namespaces.get(ns) {
                for name in tool_names {
                    if let Some(tool) = self.tools.get(name) {
                        allowed.insert(name.clone(), tool.clone());
                    }
                }
            }
        }
        allowed
    }

    pub fn get(&self, namespace: &str, name: &str) -> Option<Arc<dyn Tool>> {
        let full_name = format!("{namespace}.{name}");
        self.tools.get(&full_name).cloned()
    }
}
