import { useState, useEffect, useCallback } from "react";
import type { ToolApprovalRequestPayload } from "../protocol/types";

interface ToolApprovalModalProps {
  request: ToolApprovalRequestPayload | null;
  onRespond: (requestId: string, approved: boolean) => void;
}

const riskColors: Record<string, string> = {
  low: "oklch(70% 0.12 145)",
  medium: "oklch(75% 0.15 85)",
  high: "oklch(65% 0.2 25)",
};

const riskLabels: Record<string, string> = {
  low: "Low Risk",
  medium: "Medium Risk",
  high: "High Risk",
};

export function ToolApprovalModal({ request, onRespond }: ToolApprovalModalProps) {
  const [elapsed, setElapsed] = useState(0);

  useEffect(() => {
    if (!request) {
      setElapsed(0);
      return;
    }
    const start = Date.now();
    const interval = setInterval(() => {
      setElapsed(Math.floor((Date.now() - start) / 1000));
    }, 1000);
    return () => clearInterval(interval);
  }, [request]);

  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (!request) return;
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;
      if (e.key === "y" || e.key === "Y") {
        e.preventDefault();
        onRespond(request.request_id, true);
      } else if (e.key === "n" || e.key === "N") {
        e.preventDefault();
        onRespond(request.request_id, false);
      } else if (e.key === "Escape") {
        e.preventDefault();
        onRespond(request.request_id, false);
      }
    },
    [request, onRespond]
  );

  useEffect(() => {
    if (request) {
      window.addEventListener("keydown", handleKeyDown);
      return () => window.removeEventListener("keydown", handleKeyDown);
    }
  }, [request, handleKeyDown]);

  if (!request) return null;

  const formatElapsed = (secs: number) => {
    const m = Math.floor(secs / 60);
    const s = secs % 60;
    return m > 0 ? `${m}m ${s}s` : `${s}s`;
  };

  return (
    <div
      className="approval-overlay"
      onClick={() => onRespond(request.request_id, false)}
      role="alertdialog"
      aria-label="Tool approval request"
      aria-modal="true"
      aria-describedby="approval-body"
    >
      <div className="approval-dialog" onClick={(e) => e.stopPropagation()}>
        <div className="approval-header">
          <span className="approval-title">Tool Approval Required</span>
          <span className="approval-timer">{formatElapsed(elapsed)}</span>
        </div>

        <div className="approval-body" id="approval-body">
          <div className="approval-tool-name">{request.tool_name}</div>
          <div
            className="approval-risk-badge"
            style={{ color: riskColors[request.risk_level] ?? "inherit" }}
          >
            {riskLabels[request.risk_level] ?? request.risk_level}
          </div>
          <pre className="approval-input-preview">{request.input_preview}</pre>
        </div>

        <div className="approval-actions">
          <button
            className="approval-btn approval-btn-approve"
            onClick={() => onRespond(request.request_id, true)}
            aria-label={`Approve ${request.tool_name}`}
          >
            Approve <kbd>Y</kbd>
          </button>
          <button
            className="approval-btn approval-btn-deny"
            onClick={() => onRespond(request.request_id, false)}
            aria-label={`Deny ${request.tool_name}`}
          >
            Deny <kbd>N</kbd>
          </button>
        </div>
      </div>
    </div>
  );
}
