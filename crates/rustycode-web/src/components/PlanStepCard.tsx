import type { PlanStepState } from "../protocol/types";

interface PlanStepCardProps {
  step: PlanStepState;
  index: number;
  expanded: boolean;
  onToggle: () => void;
}

const statusIcon: Record<PlanStepState["status"], string> = {
  pending: "○",
  running: "▶",
  completed: "✓",
  failed: "✗",
};

export function PlanStepCard({ step, index, expanded, onToggle }: PlanStepCardProps) {
  return (
    <div className={`plan-step plan-step-${step.status}`}>
      <button
        className="plan-step-header"
        onClick={onToggle}
        aria-expanded={expanded}
      >
        <span className={`plan-step-icon plan-step-icon-${step.status}`}>
          {statusIcon[step.status]}
        </span>
        <span className="plan-step-number">{index + 1}</span>
        <span className="plan-step-name">{step.name}</span>
        {step.status === "running" && <span className="plan-step-spinner" />}
      </button>
      {expanded && (
        <div className="plan-step-detail">
          <p className="plan-step-desc">{step.description}</p>
          {step.message && (
            <p className={`plan-step-message ${step.status === "failed" ? "plan-step-error" : ""}`}>
              {step.message}
            </p>
          )}
        </div>
      )}
    </div>
  );
}
