import { useState } from "react";
import type { PlanState } from "../protocol/types";
import { PlanStepCard } from "./PlanStepCard";

interface PlanBannerProps {
  plan: PlanState;
  onApprove: (planId: string) => void;
  onReject: (planId: string) => void;
}

export function PlanBanner({ plan, onApprove, onReject }: PlanBannerProps) {
  const [expanded, setExpanded] = useState(false);
  const [expandedStep, setExpandedStep] = useState<number | null>(null);

  const completedCount = plan.steps.filter(
    (s) => s.status === "completed" || s.status === "failed"
  ).length;
  const totalSteps = plan.steps.length;
  const progressPct = totalSteps > 0 ? (completedCount / totalSteps) * 100 : 0;

  const currentStep = plan.steps.find((s) => s.status === "running");

  return (
    <div className={`plan-banner ${plan.completed ? (plan.success ? "plan-banner-success" : "plan-banner-failed") : ""}`}>
      <div className="plan-banner-header">
        <button
          className="plan-banner-toggle"
          onClick={() => setExpanded((e) => !e)}
          aria-expanded={expanded}
          aria-label={expanded ? "Collapse plan" : "Expand plan"}
        >
          <span className="plan-banner-chevron">{expanded ? "▼" : "▶"}</span>
          <span className="plan-banner-title">{plan.title}</span>
        </button>

        <div className="plan-banner-meta">
          {!plan.completed && !plan.awaitingApproval && currentStep && (
            <span className="plan-banner-current">
              {completedCount + 1}/{totalSteps}: {currentStep.name}
            </span>
          )}
          {plan.completed && (
            <span className={`plan-banner-status ${plan.success ? "plan-banner-status-ok" : "plan-banner-status-err"}`}>
              {plan.success ? "Completed" : "Failed"}
            </span>
          )}
          <span className="plan-banner-progress-label">
            {completedCount}/{totalSteps}
          </span>
        </div>
      </div>

      <div className="plan-banner-progress">
        <div
          className={`plan-banner-progress-bar ${plan.success ? "plan-progress-success" : plan.completed ? "plan-progress-failed" : ""}`}
          style={{ width: `${progressPct}%` }}
        />
      </div>

      {expanded && (
        <div className="plan-banner-steps">
          {plan.steps.map((step, i) => (
            <PlanStepCard
              key={i}
              step={step}
              index={i}
              expanded={expandedStep === i}
              onToggle={() => setExpandedStep(expandedStep === i ? null : i)}
            />
          ))}
          {plan.summary && (
            <div className="plan-banner-summary">
              {plan.summary}
            </div>
          )}
        </div>
      )}

      {plan.awaitingApproval && (
        <div className="plan-banner-approval">
          <p className="plan-banner-approval-text">This plan needs your approval to proceed.</p>
          <div className="plan-banner-approval-actions">
            <button
              className="plan-btn plan-btn-approve"
              onClick={() => onApprove(plan.id)}
              aria-label="Approve plan"
            >
              Approve
            </button>
            <button
              className="plan-btn plan-btn-reject"
              onClick={() => onReject(plan.id)}
              aria-label="Reject plan"
            >
              Reject
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
