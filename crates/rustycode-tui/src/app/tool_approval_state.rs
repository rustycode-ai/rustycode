//! Tool approval state.
//!
//! Groups the tool approval manager, pending requests queue, and the
//! awaiting-approval flag so approval concerns have a single owner.

use crate::tool_approval::{ApprovalRequest, ToolApprovalManager};
use std::collections::VecDeque;

/// State for the tool approval flow: pending requests, the manager that
/// processes them, and whether the UI is blocked waiting for a response.
#[derive(Debug)]
pub struct ToolApprovalState {
    /// Manages tool approval policies and decisions
    pub manager: ToolApprovalManager,
    /// Queue of requests awaiting user approval
    pub pending_requests: VecDeque<ApprovalRequest>,
    /// Whether the UI is blocked waiting for user response
    pub awaiting: bool,
}

impl ToolApprovalState {
    pub fn new(manager: ToolApprovalManager) -> Self {
        Self {
            manager,
            pending_requests: VecDeque::new(),
            awaiting: false,
        }
    }

    /// Clear all pending state after dismissal.
    pub fn clear(&mut self) {
        self.pending_requests.clear();
        self.awaiting = false;
    }

    /// Pop the next pending request and update the awaiting flag.
    ///
    /// Returns `None` if the queue is empty.
    pub fn pop_next(&mut self) -> Option<ApprovalRequest> {
        let req = self.pending_requests.pop_front();
        self.awaiting = !self.pending_requests.is_empty();
        req
    }

    /// Dismiss the current request without returning it.
    ///
    /// Updates the awaiting flag to reflect the new queue state.
    pub fn dismiss_current(&mut self) {
        self.pending_requests.pop_front();
        self.awaiting = !self.pending_requests.is_empty();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool_approval::{ApprovalRequest, ApprovalState, ToolApprovalManager};
    use std::collections::VecDeque;

    fn make_request(name: &str) -> ApprovalRequest {
        ApprovalRequest::new(
            name.into(),
            crate::tool_approval::risk::ToolType::Bash,
            format!("desc for {name}"),
            "ls".into(),
        )
    }

    #[test]
    fn new_is_empty() {
        let state = ToolApprovalState::new(ToolApprovalManager::new());
        assert!(!state.awaiting);
        assert!(state.pending_requests.is_empty());
    }

    #[test]
    fn pop_next_returns_front_and_updates_awaiting() {
        let mut state = ToolApprovalState::new(ToolApprovalManager::new());
        state.pending_requests.push_back(make_request("tool_a"));
        state.pending_requests.push_back(make_request("tool_b"));
        state.awaiting = true;

        let req = state.pop_next().unwrap();
        assert_eq!(req.tool_name, "tool_a");
        assert!(state.awaiting); // still has tool_b

        let req = state.pop_next().unwrap();
        assert_eq!(req.tool_name, "tool_b");
        assert!(!state.awaiting); // now empty

        assert!(state.pop_next().is_none());
    }

    #[test]
    fn dismiss_current_drops_without_returning() {
        let mut state = ToolApprovalState::new(ToolApprovalManager::new());
        state.pending_requests.push_back(make_request("tool_a"));
        state.pending_requests.push_back(make_request("tool_b"));
        state.awaiting = true;

        state.dismiss_current();
        assert!(state.awaiting); // still has tool_b
        assert_eq!(state.pending_requests.len(), 1);
    }

    #[test]
    fn clear_empties_everything() {
        let mut state = ToolApprovalState::new(ToolApprovalManager::new());
        state.pending_requests.push_back(make_request("tool_a"));
        state.awaiting = true;
        state.clear();
        assert!(!state.awaiting);
        assert!(state.pending_requests.is_empty());
    }
}
