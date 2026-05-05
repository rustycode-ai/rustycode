//! Tool approval request handlers.

use crate::app::TUI;
use crate::tool_approval::risk;

pub(super) fn handle_approval_request_chunk(
    tui: &mut TUI,
    tool_name: String,
    _tool_id: String,
    description: String,
    diff: Option<String>,
) {
    let tool_type = risk::classify_tool_type(&tool_name);
    let command = diff
        .clone()
        .unwrap_or_else(|| format!("Execute {}", tool_name));
    let risk_level = risk::classify_tool_risk(&tool_type, &command);

    tracing::info!(
        "TUI approval handler: {} (type={:?}, risk={:?}, ai_mode={:?})",
        tool_name,
        tool_type,
        risk_level,
        tui.services.ai_mode()
    );

    if tui.services.ai_mode() == crate::agent_mode::AiMode::Yolo {
        match risk_level {
            risk::RiskLevel::Safe => {}
            risk::RiskLevel::Medium | risk::RiskLevel::High => {
                tracing::info!("Yolo auto-approved ({:?}): {}", risk_level, tool_name);
            }
            risk::RiskLevel::Dangerous => {
                tracing::warn!("Yolo auto-approved (DESTRUCTIVE): {}", tool_name);
            }
        }
        tui.services.send_approval_response(true);
        tui.dirty = true;
        return;
    }

    // Plan mode gate: reject tools that are not allowed during planning
    if tui.plan_mode.current_phase() == "planning" {
        let plan_blocked = match tui.plan_mode.is_tool_allowed(&tool_name) {
            Ok(()) => false,
            Err(_reason) => {
                // Allow write_file for doc extensions even in plan mode
                const DOC_EXTENSIONS: &[&str] = &[".md", ".txt", ".rst", ".adoc", ".doc", ".docx"];
                if tool_name == "write_file" {
                    if let Some(path) = diff.as_ref().and_then(|d| {
                        // Try to extract path from diff string like "write_file: path=..."
                        d.split("path=")
                            .nth(1)
                            .and_then(|s| s.split_whitespace().next())
                    }) {
                        let lower = path.to_lowercase();
                        !DOC_EXTENSIONS.iter().any(|ext| lower.ends_with(ext))
                    } else {
                        true
                    }
                } else {
                    true
                }
            }
        };

        if plan_blocked {
            tui.services.send_approval_response(false);
            tui.add_system_message(format!("Plan mode blocked tool: {}", tool_name));
            tui.dirty = true;
            return;
        }
    }

    if !tui.tool_approval.requires_approval(&tool_name, risk_level) {
        tracing::info!(
            "TUI approval: {} auto-approved (safe or session-approved)",
            tool_name
        );
        tui.services.send_approval_response(true);
        tui.dirty = true;
        return;
    }

    // Check if tool has been blocked for this session
    if tui.tool_approval.is_blocked(&tool_name) {
        tracing::info!("TUI approval: {} auto-rejected (blocked)", tool_name);
        tui.services.send_approval_response(false);
        tui.add_system_message(format!("✗ Auto-rejected (blocked): {}", tool_name));
        tui.dirty = true;
        return;
    }

    // If we're already awaiting approval for the same tool, the provider sent
    // a duplicate request (race condition / retry). Auto-approve to avoid
    // deadlock — the provider is waiting for a response and won't proceed
    // without one.
    if tui.awaiting_approval {
        if let Some(req) = tui.pending_approval_request.front() {
            if req.tool_name == tool_name {
                tui.services.send_approval_response(true);
                tui.dirty = true;
                return;
            }
        }
    }

    tui.pending_approval_request
        .push_back(crate::tool_approval::ApprovalRequest {
            tool_name: tool_name.clone(),
            tool_type,
            risk_level,
            description,
            command,
            state: crate::tool_approval::ApprovalState::Pending,
        });
    tui.awaiting_approval = true;
    tui.dirty = true;
    tracing::warn!(
        "TUI approval: SHOWING PROMPT for {} (risk={:?})",
        tool_name,
        risk_level
    );
}

pub(super) fn handle_approval_approved_chunk(tui: &mut TUI, _tool_id: String) {
    if let Some(mut request) = tui.pending_approval_request.pop_front() {
        request.approve();
        tui.tool_approval
            .record_approval(request.tool_name.clone(), request.state);
        tui.add_system_message(format!("✓ Approved: {}", request.tool_name));
    }
    tui.awaiting_approval = !tui.pending_approval_request.is_empty();
    tui.dirty = true;
}

pub(super) fn handle_approval_rejected_chunk(tui: &mut TUI, _tool_id: String) {
    if let Some(mut request) = tui.pending_approval_request.pop_front() {
        request.reject();
        tui.add_system_message(format!("✗ Rejected: {}", request.tool_name));
    }
    tui.awaiting_approval = !tui.pending_approval_request.is_empty();
    tui.dirty = true;
}
