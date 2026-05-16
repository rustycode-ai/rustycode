import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, cleanup } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { PlanBanner } from "../PlanBanner";
import type { PlanState } from "../../protocol/types";

afterEach(cleanup);

function makePlan(overrides: Partial<PlanState> = {}): PlanState {
  return {
    id: "plan-1",
    title: "Refactor auth module",
    steps: [
      { name: "Read existing code", description: "Read auth.rs", status: "completed" },
      { name: "Write tests", description: "Add unit tests", status: "running" },
      { name: "Implement changes", description: "Refactor", status: "pending" },
    ],
    completed: false,
    success: false,
    awaitingApproval: false,
    ...overrides,
  };
}

describe("PlanBanner", () => {
  it("renders plan title", () => {
    render(<PlanBanner plan={makePlan()} onApprove={vi.fn()} onReject={vi.fn()} />);
    expect(screen.getByText("Refactor auth module")).toBeInTheDocument();
  });

  it("shows progress count", () => {
    render(<PlanBanner plan={makePlan()} onApprove={vi.fn()} onReject={vi.fn()} />);
    expect(screen.getByText("1/3")).toBeInTheDocument();
  });

  it("shows current running step", () => {
    render(<PlanBanner plan={makePlan()} onApprove={vi.fn()} onReject={vi.fn()} />);
    expect(screen.getByText("2/3: Write tests")).toBeInTheDocument();
  });

  it("does not show current step when all completed", () => {
    render(
      <PlanBanner
        plan={makePlan({
          steps: [
            { name: "Step 1", description: "d1", status: "completed" },
            { name: "Step 2", description: "d2", status: "completed" },
          ],
          completed: true,
          success: true,
        })}
        onApprove={vi.fn()}
        onReject={vi.fn()}
      />
    );
    expect(screen.queryByText(/\/\d+:/)).not.toBeInTheDocument();
  });

  it("shows Completed status when plan succeeds", () => {
    render(
      <PlanBanner
        plan={makePlan({ completed: true, success: true })}
        onApprove={vi.fn()}
        onReject={vi.fn()}
      />
    );
    expect(screen.getByText("Completed")).toBeInTheDocument();
  });

  it("shows Failed status when plan fails", () => {
    render(
      <PlanBanner
        plan={makePlan({ completed: true, success: false })}
        onApprove={vi.fn()}
        onReject={vi.fn()}
      />
    );
    expect(screen.getByText("Failed")).toBeInTheDocument();
  });

  it("expands steps on toggle click", async () => {
    render(<PlanBanner plan={makePlan()} onApprove={vi.fn()} onReject={vi.fn()} />);
    const toggle = screen.getByRole("button", { name: /Expand plan/ });
    expect(screen.queryByText("Read existing code")).not.toBeInTheDocument();
    await userEvent.click(toggle);
    expect(screen.getByText("Read existing code")).toBeInTheDocument();
    expect(screen.getByText("Write tests")).toBeInTheDocument();
    expect(screen.getByText("Implement changes")).toBeInTheDocument();
  });

  it("collapses steps on second toggle click", async () => {
    render(<PlanBanner plan={makePlan()} onApprove={vi.fn()} onReject={vi.fn()} />);
    const toggle = screen.getByRole("button", { name: /Expand plan/ });
    await userEvent.click(toggle);
    expect(screen.getByText("Read existing code")).toBeInTheDocument();
    await userEvent.click(toggle);
    expect(screen.queryByText("Read existing code")).not.toBeInTheDocument();
  });

  it("sets aria-expanded on toggle button", async () => {
    render(<PlanBanner plan={makePlan()} onApprove={vi.fn()} onReject={vi.fn()} />);
    const toggle = screen.getByRole("button", { name: /Expand plan/ });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    await userEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
  });

  it("shows approval buttons when awaitingApproval is true", () => {
    render(
      <PlanBanner
        plan={makePlan({ awaitingApproval: true })}
        onApprove={vi.fn()}
        onReject={vi.fn()}
      />
    );
    expect(screen.getByRole("button", { name: "Approve plan" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Reject plan" })).toBeInTheDocument();
  });

  it("does not show approval buttons by default", () => {
    render(<PlanBanner plan={makePlan()} onApprove={vi.fn()} onReject={vi.fn()} />);
    expect(screen.queryByRole("button", { name: "Approve plan" })).not.toBeInTheDocument();
  });

  it("calls onApprove with plan id", async () => {
    const onApprove = vi.fn();
    render(
      <PlanBanner
        plan={makePlan({ awaitingApproval: true })}
        onApprove={onApprove}
        onReject={vi.fn()}
      />
    );
    await userEvent.click(screen.getByRole("button", { name: "Approve plan" }));
    expect(onApprove).toHaveBeenCalledWith("plan-1");
  });

  it("calls onReject with plan id", async () => {
    const onReject = vi.fn();
    render(
      <PlanBanner
        plan={makePlan({ awaitingApproval: true })}
        onApprove={vi.fn()}
        onReject={onReject}
      />
    );
    await userEvent.click(screen.getByRole("button", { name: "Reject plan" }));
    expect(onReject).toHaveBeenCalledWith("plan-1");
  });

  it("renders progressbar with correct aria attributes", () => {
    render(<PlanBanner plan={makePlan()} onApprove={vi.fn()} onReject={vi.fn()} />);
    const bar = screen.getByRole("progressbar");
    expect(bar).toHaveAttribute("aria-valuenow", "1");
    expect(bar).toHaveAttribute("aria-valuemin", "0");
    expect(bar).toHaveAttribute("aria-valuemax", "3");
  });

  it("shows summary when expanded and present", async () => {
    render(
      <PlanBanner
        plan={makePlan({ summary: "All done!", completed: true, success: true })}
        onApprove={vi.fn()}
        onReject={vi.fn()}
      />
    );
    await userEvent.click(screen.getByRole("button", { name: /Expand plan/ }));
    expect(screen.getByText("All done!")).toBeInTheDocument();
  });
});
