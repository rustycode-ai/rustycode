//! Tool approval request handlers.

use crate::app::TUI;
use crate::tool_approval::risk;
use rustycode_protocol::{tool_names as tn, Op};

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
        tui.integration.services.ai_mode()
    );

    if tui.integration.services.ai_mode() == crate::services::agent_mode::AiMode::Yolo {
        match risk_level {
            risk::RiskLevel::Safe => {}
            risk::RiskLevel::Medium | risk::RiskLevel::High => {
                tracing::info!("Yolo auto-approved ({:?}): {}", risk_level, tool_name);
            }
            risk::RiskLevel::Dangerous => {
                tracing::warn!("Yolo auto-approved (DESTRUCTIVE): {}", tool_name);
            }
        }
        tui.integration
            .services
            .submit_op(Op::ApproveTool { approved: true })
            .ok();
        tui.sys.dirty = true;
        return;
    }

    // Plan mode gate: reject tools that are not allowed during planning
    if tui.model.plan_mode.current_phase() == "planning" {
        let plan_blocked = match tui.model.plan_mode.is_tool_allowed(&tool_name) {
            Ok(()) => false,
            Err(_reason) => {
                // Allow write_file for doc extensions even in plan mode
                const DOC_EXTENSIONS: &[&str] = &[".md", ".txt", ".rst", ".adoc", ".doc", ".docx"];
                if tool_name == tn::WRITE {
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
            tui.integration
                .services
                .submit_op(Op::ApproveTool { approved: false })
                .ok();
            tui.add_system_message(format!("Plan mode blocked tool: {}", tool_name));
            tui.sys.dirty = true;
            return;
        }
    }

    if !tui
        .panels
        .tool_approval
        .manager
        .requires_approval(&tool_name, risk_level)
    {
        tracing::info!(
            "TUI approval: {} auto-approved (safe or session-approved)",
            tool_name
        );
        tui.integration
            .services
            .submit_op(Op::ApproveTool { approved: true })
            .ok();
        tui.sys.dirty = true;
        return;
    }

    // Check if tool has been blocked for this session
    if tui.panels.tool_approval.manager.is_blocked(&tool_name) {
        tracing::info!("TUI approval: {} auto-rejected (blocked)", tool_name);
        tui.integration
            .services
            .submit_op(Op::ApproveTool { approved: false })
            .ok();
        tui.add_system_message(format!("✗ Auto-rejected (blocked): {}", tool_name));
        tui.sys.dirty = true;
        return;
    }

    // If we're already awaiting approval for the same tool, the provider sent
    // a duplicate request (race condition / retry). Auto-approve to avoid
    // deadlock — the provider is waiting for a response and won't proceed
    // without one.
    if tui.panels.tool_approval.awaiting {
        if let Some(req) = tui.panels.tool_approval.pending_requests.front() {
            if req.tool_name == tool_name {
                tui.integration
                    .services
                    .submit_op(Op::ApproveTool { approved: true })
                    .ok();
                tui.sys.dirty = true;
                return;
            }
        }
    }

    // PermissionRequest hook: allow hooks to deny before showing approval dialog.
    let hook_result = tui.integration.hook_manager.execute_blocking(
        rustycode_tools::hooks::HookTrigger::PermissionRequest,
        serde_json::json!({
            "tool_name": tool_name,
            "args": description,
            "risk_level": format!("{:?}", risk_level),
        }),
    );
    if hook_result.should_block {
        tui.integration.services.send_approval_response(false);
        tui.add_system_message(format!(
            "✗ Hook blocked: {} ({})",
            tool_name,
            hook_result
                .block_reason
                .as_deref()
                .unwrap_or("blocked by hook")
        ));
        tui.sys.dirty = true;
        return;
    }

    tui.panels
        .tool_approval
        .pending_requests
        .push_back(crate::tool_approval::ApprovalRequest {
            tool_name: tool_name.clone(),
            tool_type,
            risk_level,
            description,
            command,
            state: crate::tool_approval::ApprovalState::Pending,
            diff_scroll: crate::tool_approval::DiffScrollState::default(),
        });
    tui.panels.tool_approval.awaiting = true;
    tui.sys.dirty = true;
    tracing::warn!(
        "TUI approval: SHOWING PROMPT for {} (risk={:?})",
        tool_name,
        risk_level
    );
}

pub(super) fn handle_approval_approved_chunk(tui: &mut TUI, _tool_id: String) {
    if let Some(mut request) = tui.panels.tool_approval.pop_next() {
        request.approve();
        tui.panels
            .tool_approval
            .manager
            .record_approval(request.tool_name.clone(), request.state);
        tui.add_system_message(format!("✓ Approved: {}", request.tool_name));
    }
    tui.sys.dirty = true;
}

pub(super) fn handle_approval_rejected_chunk(tui: &mut TUI, _tool_id: String) {
    if let Some(mut request) = tui.panels.tool_approval.pop_next() {
        request.reject();
        tui.add_system_message(format!("✗ Rejected: {}", request.tool_name));
    }
    tui.sys.dirty = true;
}
