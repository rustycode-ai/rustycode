//! Tool panel feature module
//!
//! Handles tool execution state and approval panel UI.
//! Owns tool-specific state (pending approvals, execution results, etc.)
//!
//! ## State
//! - `ToolPanelState`: Tracks pending tool approvals, execution results, status
//!
//! ## Events Handled
//! - `TuiEvent::Service(EventMsg)`: Tool approval requests and execution updates
//! - `TuiEvent::Stream(StreamChunk)`: Tool execution start/progress/complete signals
//! - `TuiEvent::Key`: Approve/Reject tool execution (when panel has focus)
//!
//! ## Surfaces
//! - "tool-panel": Approval panel and tool execution display
//! - "tool-approval": Modal for tool approval confirmation
//!
//! ## Rendering
//! Renders pending tools list and approval modal UI

use crate::app::features::{
    FeatureRegistry, RenderCtx, SurfaceId, TuiAction, TuiEvent, TuiFeature, UpdateCtx,
};
use chrono::{DateTime, Utc};
use ratatui::Frame;

/// Pending tool approval state
#[derive(Debug, Clone)]
pub struct PendingApproval {
    /// Tool ID from tool use block
    pub tool_id: String,
    /// Tool name
    pub tool_name: String,
    /// Tool parameters (from input)
    pub parameters: String,
    /// When approval was requested
    pub requested_at: DateTime<Utc>,
    /// Whether user is being asked to approve
    pub awaiting_user_decision: bool,
}

/// Tool execution result
#[derive(Debug, Clone)]
pub struct ToolResult {
    /// Tool ID that was executed
    pub tool_id: String,
    /// Tool name
    pub tool_name: String,
    /// Execution result (success or error)
    pub result: String,
    /// Whether execution succeeded
    pub success: bool,
    /// When result was received
    pub completed_at: DateTime<Utc>,
}

/// Tool panel state management
#[derive(Default)]
pub struct ToolPanelState {
    /// Pending tool approvals awaiting user decision
    pub pending_approvals: Vec<PendingApproval>,
    /// Recently executed tools and their results
    pub tool_results: Vec<ToolResult>,
    /// Whether tool approval panel is currently visible
    pub approval_panel_visible: bool,
    /// Currently focused approval in the list (for keyboard navigation)
    pub selected_approval: usize,
    /// Maximum recent results to keep in history
    pub max_results_history: usize,
}

impl ToolPanelState {
    /// Create a new tool panel state
    pub fn new() -> Self {
        Self {
            pending_approvals: Vec::new(),
            tool_results: Vec::new(),
            approval_panel_visible: false,
            selected_approval: 0,
            max_results_history: 10,
        }
    }

    /// Add a pending approval request
    pub fn request_approval(
        &mut self,
        tool_id: impl Into<String>,
        tool_name: impl Into<String>,
        parameters: impl Into<String>,
    ) {
        let approval = PendingApproval {
            tool_id: tool_id.into(),
            tool_name: tool_name.into(),
            parameters: parameters.into(),
            requested_at: Utc::now(),
            awaiting_user_decision: true,
        };
        self.pending_approvals.push(approval);
        self.approval_panel_visible = true;
        if self.selected_approval >= self.pending_approvals.len() {
            self.selected_approval = self.pending_approvals.len().saturating_sub(1);
        }
    }

    /// Approve the currently selected tool
    pub fn approve_selected(&mut self) -> Option<String> {
        if let Some(approval) = self.pending_approvals.get_mut(self.selected_approval) {
            let tool_id = approval.tool_id.clone();
            self.pending_approvals.remove(self.selected_approval);
            if self.selected_approval >= self.pending_approvals.len() {
                self.selected_approval = self.pending_approvals.len().saturating_sub(1);
            }
            return Some(tool_id);
        }
        None
    }

    /// Reject the currently selected tool
    pub fn reject_selected(&mut self) -> Option<String> {
        if let Some(approval) = self.pending_approvals.get_mut(self.selected_approval) {
            let tool_id = approval.tool_id.clone();
            self.pending_approvals.remove(self.selected_approval);
            if self.selected_approval >= self.pending_approvals.len() {
                self.selected_approval = self.pending_approvals.len().saturating_sub(1);
            }
            return Some(tool_id);
        }
        None
    }

    /// Get the currently selected approval
    pub fn current_approval(&self) -> Option<&PendingApproval> {
        self.pending_approvals.get(self.selected_approval)
    }

    /// Record a tool execution result
    pub fn record_result(
        &mut self,
        tool_id: impl Into<String>,
        tool_name: impl Into<String>,
        result: impl Into<String>,
        success: bool,
    ) {
        let tool_result = ToolResult {
            tool_id: tool_id.into(),
            tool_name: tool_name.into(),
            result: result.into(),
            success,
            completed_at: Utc::now(),
        };
        self.tool_results.push(tool_result);

        if self.tool_results.len() > self.max_results_history {
            self.tool_results.remove(0);
        }
    }

    /// Clear all pending approvals and results
    pub fn reset(&mut self) {
        self.pending_approvals.clear();
        self.tool_results.clear();
        self.approval_panel_visible = false;
        self.selected_approval = 0;
    }

    /// Get count of pending approvals
    pub fn pending_count(&self) -> usize {
        self.pending_approvals.len()
    }

    /// Select next approval
    pub fn select_next_approval(&mut self) {
        if !self.pending_approvals.is_empty()
            && self.selected_approval < self.pending_approvals.len() - 1
        {
            self.selected_approval += 1;
        }
    }

    /// Select previous approval
    pub fn select_previous_approval(&mut self) {
        if self.selected_approval > 0 {
            self.selected_approval -= 1;
        }
    }
}

/// Tool panel feature for tool execution and approval management
pub struct ToolPanelFeature {
    state: ToolPanelState,
    panel_surface: SurfaceId,
    approval_surface: SurfaceId,
}

impl ToolPanelFeature {
    /// Create a new tool panel feature
    pub fn new() -> Self {
        Self {
            state: ToolPanelState::new(),
            panel_surface: SurfaceId::new("tool-panel"),
            approval_surface: SurfaceId::new("tool-approval"),
        }
    }
}

impl Default for ToolPanelFeature {
    fn default() -> Self {
        Self::new()
    }
}

impl TuiFeature for ToolPanelFeature {
    fn id(&self) -> &'static str {
        "tool-panel"
    }

    fn register(&self, reg: &mut FeatureRegistry) {
        reg.register_surface(self.panel_surface, self.id());
        reg.register_surface(self.approval_surface, self.id());
    }

    fn update(&mut self, event: &TuiEvent, _ctx: &mut UpdateCtx) -> Vec<TuiAction> {
        match event {
            TuiEvent::Service(_event_msg) => {
                // TODO: Handle tool approval requests and completion events
                // - ToolApprovalRequest: request_approval()
                // - ToolExecutionComplete: record_result()
                Vec::new()
            }
            TuiEvent::Stream(_chunk) => {
                // TODO: Handle tool-related stream chunks
                // - StreamChunk::ToolStart: signal tool execution started
                // - StreamChunk::ToolProgress: update progress
                // - StreamChunk::ToolComplete: record result
                Vec::new()
            }
            TuiEvent::Key(_key) => {
                // TODO: Handle keyboard navigation in approval panel
                // - Up/Down: select_next/prev_approval()
                // - Enter: approve_selected() → emit ToolApproved action
                // - 'r': reject_selected() → emit ToolRejected action
                // - Esc: close approval panel
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn render(&self, surface: SurfaceId, _frame: &mut Frame, _ctx: &RenderCtx) {
        match surface {
            s if s == self.panel_surface => {
                // TODO: Render tool execution results history
            }
            s if s == self.approval_surface => {
                // TODO: Render tool approval modal
            }
            _ => {}
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_panel_state_new_is_empty() {
        let state = ToolPanelState::new();
        assert_eq!(state.pending_approvals.len(), 0);
        assert_eq!(state.tool_results.len(), 0);
        assert!(!state.approval_panel_visible);
    }

    #[test]
    fn tool_panel_state_requests_approval() {
        let mut state = ToolPanelState::new();
        state.request_approval("tool-1", "echo", "hello");

        assert_eq!(state.pending_approvals.len(), 1);
        assert!(state.approval_panel_visible);
        assert_eq!(state.pending_approvals[0].tool_id, "tool-1");
    }

    #[test]
    fn tool_panel_state_approves_tool() {
        let mut state = ToolPanelState::new();
        state.request_approval("tool-1", "echo", "hello");

        let approved_id = state.approve_selected();
        assert_eq!(approved_id, Some("tool-1".to_string()));
        assert_eq!(state.pending_approvals.len(), 0);
    }

    #[test]
    fn tool_panel_state_rejects_tool() {
        let mut state = ToolPanelState::new();
        state.request_approval("tool-1", "echo", "hello");

        let rejected_id = state.reject_selected();
        assert_eq!(rejected_id, Some("tool-1".to_string()));
        assert_eq!(state.pending_approvals.len(), 0);
    }

    #[test]
    fn tool_panel_state_records_results() {
        let mut state = ToolPanelState::new();
        state.record_result("tool-1", "echo", "output", true);
        state.record_result("tool-2", "bash", "error", false);

        assert_eq!(state.tool_results.len(), 2);
        assert!(state.tool_results[0].success);
        assert!(!state.tool_results[1].success);
    }

    #[test]
    fn tool_panel_feature_has_id() {
        let feature = ToolPanelFeature::new();
        assert_eq!(feature.id(), "tool-panel");
    }

    #[test]
    fn tool_panel_feature_registers_surfaces() {
        let feature = ToolPanelFeature::new();
        let mut reg = crate::app::features::FeatureRegistry::new();
        feature.register(&mut reg);

        assert_eq!(
            reg.surface_feature(SurfaceId::new("tool-panel")),
            Some("tool-panel")
        );
        assert_eq!(
            reg.surface_feature(SurfaceId::new("tool-approval")),
            Some("tool-panel")
        );
    }

    #[test]
    fn tool_panel_state_navigation() {
        let mut state = ToolPanelState::new();
        state.request_approval("tool-1", "echo", "hello");
        state.request_approval("tool-2", "bash", "world");

        assert_eq!(state.selected_approval, 1);

        state.select_previous_approval();
        assert_eq!(state.selected_approval, 0);

        state.select_next_approval();
        assert_eq!(state.selected_approval, 1);
    }

    #[test]
    fn tool_panel_state_resets_all() {
        let mut state = ToolPanelState::new();
        state.request_approval("tool-1", "echo", "hello");
        state.record_result("tool-2", "bash", "output", true);

        state.reset();

        assert_eq!(state.pending_approvals.len(), 0);
        assert_eq!(state.tool_results.len(), 0);
        assert!(!state.approval_panel_visible);
    }
}
