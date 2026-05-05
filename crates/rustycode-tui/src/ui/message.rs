//! Message hierarchy with collapsible tool executions.
//!
//! Re-exports from `message_types` (core types) and `message_renderer` (rendering).
//! Tool helpers live in the private `message_tools_impl` module below.

// Re-export all public items from sibling modules
pub use crate::ui::message_renderer::{MessageRenderer, MessageTheme};
pub use crate::ui::message_types::{
    ExpansionLevel, ImageAttachment, Message, MessageRole, ToolExecution, ToolStatus,
};

// Include tool helpers as a private module
mod message_tools_impl {
    use crate::ui::message_types::{ExpansionLevel, Message, ToolExecution, ToolStatus};

    impl Message {
        pub fn update_tool_status(&mut self, index: usize, status: ToolStatus) {
            if let Some(ref mut tools) = self.tool_executions {
                if index < tools.len() {
                    tools[index].status = status;
                }
            }
        }

        pub fn add_tool(&mut self, tool: ToolExecution) {
            match &mut self.tool_executions {
                Some(tools) => tools.push(tool),
                None => self.tool_executions = Some(vec![tool]),
            }
        }

        pub fn tool(&self, index: usize) -> Option<&ToolExecution> {
            self.tool_executions
                .as_ref()
                .and_then(|tools| tools.get(index))
        }

        pub fn tool_mut(&mut self, index: usize) -> Option<&mut ToolExecution> {
            self.tool_executions
                .as_mut()
                .and_then(|tools| tools.get_mut(index))
        }

        pub fn all_tools_complete(&self) -> bool {
            self.tool_executions
                .as_ref()
                .map(|tools| {
                    tools.is_empty()
                        || tools.iter().all(|t| {
                            t.status == ToolStatus::Complete || t.status == ToolStatus::Failed
                        })
                })
                .unwrap_or(true)
        }

        pub fn has_running_tools(&self) -> bool {
            self.tool_executions
                .as_ref()
                .map(|tools| tools.iter().any(|t| t.status == ToolStatus::Running))
                .unwrap_or(false)
        }

        pub fn first_running_tool_index(&self) -> Option<usize> {
            self.tool_executions
                .as_ref()
                .and_then(|tools| tools.iter().position(|t| t.status == ToolStatus::Running))
        }

        pub fn clear_tools(&mut self) {
            self.tool_executions = None;
            self.tools_expansion = ExpansionLevel::Collapsed;
            self.focused_tool_index = None;
        }
    }

    impl ToolExecution {
        pub fn completed(name: String, result_summary: String, detailed_output: String) -> Self {
            let mut tool = Self::new("tool".to_string(), name, result_summary);
            tool.complete(Some(detailed_output));
            tool
        }

        pub fn failed(name: String, error: String) -> Self {
            let mut tool = Self::new("tool".to_string(), name, String::new());
            tool.fail(error);
            tool
        }

        pub fn is_running(&self) -> bool {
            self.status == ToolStatus::Running
        }

        pub fn is_complete(&self) -> bool {
            self.status == ToolStatus::Complete
        }

        pub fn is_failed(&self) -> bool {
            self.status == ToolStatus::Failed
        }

        pub fn is_finished(&self) -> bool {
            self.is_complete() || self.is_failed()
        }

        pub fn elapsed_ms(&self) -> Option<u64> {
            if let Some(end_time) = self.end_time {
                Some(
                    end_time
                        .signed_duration_since(self.start_time)
                        .num_milliseconds()
                        .max(0) as u64,
                )
            } else {
                Some(
                    chrono::Utc::now()
                        .signed_duration_since(self.start_time)
                        .num_milliseconds()
                        .max(0) as u64,
                )
            }
        }
    }
}
