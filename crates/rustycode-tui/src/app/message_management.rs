//! Message management utilities
//!
//! Handles adding, updating, and managing messages in the conversation.

use crate::ui::message::{Message, MessageRole};

/// Add a system message to the conversation
pub fn add_system_message(messages: &mut Vec<Message>, content: String, dirty: &mut bool) {
    let message = Message::system(content);
    messages.push(message);
    *dirty = true;
}

/// Add an assistant (AI) message to the conversation
pub fn add_ai_message(messages: &mut Vec<Message>, content: String, dirty: &mut bool) {
    let message = Message::assistant(content);
    messages.push(message);
    *dirty = true;
}

/// Add a user message to the conversation
pub fn add_user_message(messages: &mut Vec<Message>, content: String, dirty: &mut bool) {
    let message = Message::user(content);
    messages.push(message);
    *dirty = true;
}

/// Add tools to the last assistant message
pub fn add_tools_to_last_message(
    messages: &mut [Message],
    tools: Vec<crate::ui::message::ToolExecution>,
    dirty: &mut bool,
) {
    if let Some(last_msg) = messages.last_mut() {
        if last_msg.role == MessageRole::Assistant {
            last_msg.tool_executions = Some(tools);
            *dirty = true;
        }
    }
}

/// Add thinking content to the last assistant message
pub fn add_thinking_to_last_message(messages: &mut [Message], thinking: String, dirty: &mut bool) {
    if let Some(last_msg) = messages.last_mut() {
        if last_msg.role == MessageRole::Assistant {
            last_msg.thinking = Some(thinking);
            *dirty = true;
        }
    }
}

/// Remove the last message and return it
pub fn pop_last_message(messages: &mut Vec<Message>, dirty: &mut bool) -> Option<Message> {
    *dirty = true;
    messages.pop()
}

/// Remove a message at the given index
pub fn remove_message(
    messages: &mut Vec<Message>,
    index: usize,
    dirty: &mut bool,
) -> Option<Message> {
    if index < messages.len() {
        *dirty = true;
        Some(messages.remove(index))
    } else {
        None
    }
}

/// Get the last assistant message index
pub fn last_assistant_message_idx(messages: &[Message]) -> Option<usize> {
    messages
        .iter()
        .rposition(|msg| msg.role == MessageRole::Assistant)
}

/// Get the last user message index
pub fn last_user_message_idx(messages: &[Message]) -> Option<usize> {
    messages
        .iter()
        .rposition(|msg| msg.role == MessageRole::User)
}

/// Clear all messages
pub fn clear_messages(messages: &mut Vec<Message>, dirty: &mut bool) {
    messages.clear();
    *dirty = true;
}

/// Update a message at the given index
pub fn update_message(
    messages: &mut [Message],
    index: usize,
    content: String,
    dirty: &mut bool,
) -> bool {
    if let Some(message) = messages.get_mut(index) {
        message.content = content;
        *dirty = true;
        true
    } else {
        false
    }
}
