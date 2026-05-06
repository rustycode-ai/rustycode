//! Tool approval state.
//!
//! Groups the tool approval manager, pending requests queue, and the
//! awaiting-approval flag so approval concerns have a single owner.

use crate::tool_approval::{ApprovalRequest, ToolApprovalManager};
use std::collections::VecDeque;

/// State for the tool approval flow: pending requests, the manager that
/// processes them, and whether the UI is blocked waiting for a response.
#[derive(Debug)]
#[non_exhaustive]
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
}
