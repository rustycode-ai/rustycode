use serde::{Deserialize, Serialize};

/// The context in which an `ExecutableUnit` is invoked
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ExecutionContext {
    DirectTool {
        immediate_result: bool,
        timeout_ms: Option<u64>,
    },
    SkillReference {
        discoverable: bool,
        cacheable: bool,
    },
    AgentReasoning {
        autonomous: bool,
        max_steps: Option<u32>,
        can_delegate: bool,
    },
    /// Programmatic call from generated code
    ProgrammaticCall {
        /// Chain position in a sequence of calls
        chain_position: Option<u32>,
        /// Whether results should be passed to the next call
        passthrough: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExecutionCapability {
    DirectExecution,
    Knowledge,
    Reasoning,
}

impl ExecutionContext {
    pub const fn requires_capability(&self) -> ExecutionCapability {
        match self {
            Self::DirectTool { .. } | Self::ProgrammaticCall { .. } => {
                ExecutionCapability::DirectExecution
            }
            Self::SkillReference { .. } => ExecutionCapability::Knowledge,
            Self::AgentReasoning { .. } => ExecutionCapability::Reasoning,
        }
    }
}
