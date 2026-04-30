//! Message hierarchy system - cleaner tool/thinking display
//!
//! This module has been split into smaller, focused modules:
//! - `message_types`: Core type definitions (Message, MessageRole, ToolExecution, etc.)
//! - `message_tools`: Tool execution helper methods
//! - `message_renderer`: Rendering logic for messages, markdown, code blocks, etc.
//!
//! ## Problem with Current Design
//!
//! **Current rustycode behavior (what user hates):**
//! - Every tool execution adds multiple messages to the main conversation
//! - Tool messages pollute the main conversation flow
//! - No clear visual hierarchy between user-facing AI content and internal tool execution
//! - Thinking messages shown inline with regular messages
//! - Multiple separate messages for related tool actions (e.g., todo list = 4 messages)
//!
//! ## Cleaner Design: Hierarchical Message Structure
//!
//! **Key improvements:**
//! 1. **Tool summary inline**: Shows "Executed: 3 tools" within the AI message
//! 2. **Expandable details**: Click [▾] to see what tools ran (collapsed by default)
//! 3. **Visual hierarchy**: Tools are a subsection of the AI message, not separate messages
//! 4. **Clean conversation**: Main flow shows user → AI → user → AI (no tool noise)
//! 5. **Grouped related actions**: All tools from one AI response shown together
//!
//! ## Display Modes
//!
//! **1. Compact Mode (default):**
//! ```text
//! ┌───────────────────────────────────────────────────────────┐
//! │ ▐ [ai] I'll help you with that.                          │
//! │ ▐                                                        │
//! │ ▐ 🔧 Executed: 3 tools                        [▾] 145b  │
//! │ ▐                                                        │
//! │ ▐ Done! Here's the result...                            │
//! └───────────────────────────────────────────────────────────┘
//! ```
//!
//! **2. Expanded Mode (press Enter on tool summary):**
//! ```text
//! ┌───────────────────────────────────────────────────────────┐
//! │ ▐ [ai] I'll help you with that.                          │
//! │ ▐                                                        │
//! │ ▐ ┌─────────────────────────────────────────────────────┐│
//! │ ▐ │ 🔧 Executed: 3 tools                        [▾]    ││
//! │ ▐ │ ┌───────────────────────────────────────────────────┤│
//! │ ▐ │ │ ✅ [1] read_file: src/main.rs           23b     ││
//! │ ▐ │ │ ✅ [2] write_file: src/tree.rs          122b    ││
//! │ ▐ │ │ ✅ [3] bash: cargo check                0.3s    ││
//! │ ▐ │ └───────────────────────────────────────────────────┘│
//! │ ▐ └─────────────────────────────────────────────────────┘│
//! │ ▐                                                        │
//! │ ▐ Done! Here's the result...                            │
//! └───────────────────────────────────────────────────────────┘
//! ```
//!
//! **3. Deep Detail (press Enter on specific tool):**
//! ```text
//! ┌───────────────────────────────────────────────────────────┐
//! │ ▐ ┌─────────────────────────────────────────────────────┐│
//! │ ▐ │ 🔧 Executed: 3 tools                        [▾]    ││
//! │ ▐ │ ┌───────────────────────────────────────────────────┤│
//! │ ▐ │ │ ✅ [1] read_file: src/main.rs           23b     ││
//! │ ▐ │ │ ┌─────────────────────────────────────────────────┤│
//! │ ▐ │ │ │ Content:                                        ││
//! │ ▐ │ │ │ fn main() {                                     ││
//! │ ▐ │ │ │     println!("Hello");                          ││
//! │ ▐ │ │ │ }                                               ││
//! │ ▐ │ │ └─────────────────────────────────────────────────┘│
//! │ ▐ │ │                                                   ││
//! │ ▐ │ │ ✅ [2] write_file: src/tree.rs          122b    ││
//! │ ▐ │ │ ✅ [3] bash: cargo check                0.3s    ││
//! │ ▐ │ └───────────────────────────────────────────────────┘│
//! │ ▐ └─────────────────────────────────────────────────────┘│
//! └───────────────────────────────────────────────────────────┘
//! ```

// Re-export all public items from sibling modules
pub use crate::ui::message_renderer::{MessageRenderer, MessageTheme};
pub use crate::ui::message_types::{
    ExpansionLevel, ImageAttachment, Message, MessageRole, ToolExecution, ToolStatus,
};

// Include tool helpers as a private module
mod message_tools_impl {
    // Tool execution helper methods
    use crate::ui::message_types::{ExpansionLevel, Message, ToolExecution, ToolStatus};

    impl Message {
        /// Update a tool's status by index
        pub fn update_tool_status(&mut self, index: usize, status: ToolStatus) {
            if let Some(ref mut tools) = self.tool_executions {
                if index < tools.len() {
                    tools[index].status = status;
                }
            }
        }

        /// Add a new tool execution to this message
        pub fn add_tool(&mut self, tool: ToolExecution) {
            match &mut self.tool_executions {
                Some(tools) => tools.push(tool),
                None => self.tool_executions = Some(vec![tool]),
            }
        }

        /// Get a reference to a tool by index
        pub fn get_tool(&self, index: usize) -> Option<&ToolExecution> {
            self.tool_executions
                .as_ref()
                .and_then(|tools| tools.get(index))
        }

        /// Get a mutable reference to a tool by index
        pub fn get_tool_mut(&mut self, index: usize) -> Option<&mut ToolExecution> {
            self.tool_executions
                .as_mut()
                .and_then(|tools| tools.get_mut(index))
        }

        /// Check if all tools are complete
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

        /// Check if any tool is currently running
        pub fn has_running_tools(&self) -> bool {
            self.tool_executions
                .as_ref()
                .map(|tools| tools.iter().any(|t| t.status == ToolStatus::Running))
                .unwrap_or(false)
        }

        /// Get the index of the first running tool, if any
        pub fn first_running_tool_index(&self) -> Option<usize> {
            self.tool_executions
                .as_ref()
                .and_then(|tools| tools.iter().position(|t| t.status == ToolStatus::Running))
        }

        /// Clear all tool executions
        pub fn clear_tools(&mut self) {
            self.tool_executions = None;
            self.tools_expansion = ExpansionLevel::Collapsed;
            self.focused_tool_index = None;
        }
    }

    impl ToolExecution {
        /// Create a completed tool execution
        pub fn completed(name: String, result_summary: String, detailed_output: String) -> Self {
            let mut tool = Self::new("tool".to_string(), name, result_summary);
            tool.complete(Some(detailed_output));
            tool
        }

        /// Create a failed tool execution
        pub fn failed(name: String, error: String) -> Self {
            let mut tool = Self::new("tool".to_string(), name, String::new());
            tool.fail(error);
            tool
        }

        /// Check if this tool is running
        pub fn is_running(&self) -> bool {
            self.status == ToolStatus::Running
        }

        /// Check if this tool completed successfully
        pub fn is_complete(&self) -> bool {
            self.status == ToolStatus::Complete
        }

        /// Check if this tool failed
        pub fn is_failed(&self) -> bool {
            self.status == ToolStatus::Failed
        }

        /// Check if this tool is finished (either complete or failed)
        pub fn is_finished(&self) -> bool {
            self.is_complete() || self.is_failed()
        }

        /// Get the elapsed duration so far (even if still running)
        pub fn elapsed_ms(&self) -> Option<u64> {
            if let Some(end_time) = self.end_time {
                Some(
                    end_time
                        .signed_duration_since(self.start_time)
                        .num_milliseconds()
                        .max(0) as u64,
                )
            } else {
                // Still running - calculate current elapsed time
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
