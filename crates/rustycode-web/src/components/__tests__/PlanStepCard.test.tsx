import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, cleanup } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { PlanStepCard } from "../PlanStepCard";
import type { PlanStepState } from "../../protocol/types";

afterEach(cleanup);

function makeStep(overrides: Partial<PlanStepState> = {}): PlanStepState {
  return {
    name: "Read files",
    description: "Read the source files",
    status: "pending",
    ...overrides,
  };
}

describe("PlanStepCard", () => {
  it("renders step name", () => {
    render(<PlanStepCard step={makeStep()} index={0} expanded={false} onToggle={() => {}} />);
    expect(screen.getByText("Read files")).toBeInTheDocument();
  });

  it("renders 1-based step number", () => {
    render(<PlanStepCard step={makeStep()} index={2} expanded={false} onToggle={() => {}} />);
    expect(screen.getByText("3")).toBeInTheDocument();
  });

  it("renders pending icon", () => {
    render(<PlanStepCard step={makeStep({ status: "pending" })} index={0} expanded={false} onToggle={() => {}} />);
    expect(screen.getByText("○")).toBeInTheDocument();
  });

  it("renders running icon", () => {
    render(<PlanStepCard step={makeStep({ status: "running" })} index={0} expanded={false} onToggle={() => {}} />);
    expect(screen.getByText("▶")).toBeInTheDocument();
  });

  it("renders completed icon", () => {
    render(<PlanStepCard step={makeStep({ status: "completed" })} index={0} expanded={false} onToggle={() => {}} />);
    expect(screen.getByText("✓")).toBeInTheDocument();
  });

  it("renders failed icon", () => {
    render(<PlanStepCard step={makeStep({ status: "failed" })} index={0} expanded={false} onToggle={() => {}} />);
    expect(screen.getByText("✗")).toBeInTheDocument();
  });

  it("shows description when expanded", () => {
    render(<PlanStepCard step={makeStep()} index={0} expanded={true} onToggle={() => {}} />);
    expect(screen.getByText("Read the source files")).toBeInTheDocument();
  });

  it("hides description when collapsed", () => {
    render(<PlanStepCard step={makeStep()} index={0} expanded={false} onToggle={() => {}} />);
    expect(screen.queryByText("Read the source files")).not.toBeInTheDocument();
  });

  it("shows message when expanded and message present", () => {
    render(
      <PlanStepCard
        step={makeStep({ message: "File not found" })}
        index={0}
        expanded={true}
        onToggle={() => {}}
      />
    );
    expect(screen.getByText("File not found")).toBeInTheDocument();
  });

  it("hides message when collapsed", () => {
    render(
      <PlanStepCard
        step={makeStep({ message: "File not found" })}
        index={0}
        expanded={false}
        onToggle={() => {}}
      />
    );
    expect(screen.queryByText("File not found")).not.toBeInTheDocument();
  });

  it("calls onToggle on header click", async () => {
    const onToggle = vi.fn();
    render(<PlanStepCard step={makeStep()} index={0} expanded={false} onToggle={onToggle} />);
    await userEvent.click(screen.getByRole("button"));
    expect(onToggle).toHaveBeenCalledTimes(1);
  });

  it("sets aria-expanded to match expanded prop", () => {
    const { rerender } = render(
      <PlanStepCard step={makeStep()} index={0} expanded={false} onToggle={() => {}} />
    );
    expect(screen.getByRole("button")).toHaveAttribute("aria-expanded", "false");

    rerender(<PlanStepCard step={makeStep()} index={0} expanded={true} onToggle={() => {}} />);
    expect(screen.getByRole("button")).toHaveAttribute("aria-expanded", "true");
  });

  it("applies status class to root element", () => {
    render(<PlanStepCard step={makeStep({ status: "running" })} index={0} expanded={false} onToggle={() => {}} />);
    expect(document.querySelector(".plan-step-running")).toBeInTheDocument();
  });

  it("shows spinner for running status", () => {
    render(<PlanStepCard step={makeStep({ status: "running" })} index={0} expanded={false} onToggle={() => {}} />);
    expect(document.querySelector(".plan-step-spinner")).toBeInTheDocument();
  });

  it("does not show spinner for non-running status", () => {
    render(<PlanStepCard step={makeStep({ status: "completed" })} index={0} expanded={false} onToggle={() => {}} />);
    expect(document.querySelector(".plan-step-spinner")).not.toBeInTheDocument();
  });

  it("applies error class to failed step message", () => {
    render(
      <PlanStepCard
        step={makeStep({ status: "failed", message: "crashed" })}
        index={0}
        expanded={true}
        onToggle={() => {}}
      />
    );
    expect(document.querySelector(".plan-step-error")).toBeInTheDocument();
  });
});
