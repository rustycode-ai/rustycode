import { describe, it, expect, afterEach } from "vitest";
import { render, screen, cleanup } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ToolCallCard } from "../ToolCallCard";
import type { ToolCallPart } from "../../protocol/types";

afterEach(cleanup);

function makePart(overrides: Partial<ToolCallPart> = {}): ToolCallPart {
  return {
    type: "tool_call",
    id: "tc-1",
    name: "bash",
    status: "completed",
    input: "echo hello",
    output: "hello",
    startedAt: 1000,
    completedAt: 1500,
    ...overrides,
  };
}

describe("ToolCallCard", () => {
  it("renders tool name", () => {
    render(<ToolCallCard part={makePart()} />);
    expect(screen.getByText("bash")).toBeInTheDocument();
  });

  it("shows checkmark for completed status", () => {
    render(<ToolCallCard part={makePart({ status: "completed" })} />);
    expect(document.querySelector(".tool-status-done")).toBeInTheDocument();
  });

  it("shows error icon for error status", () => {
    render(<ToolCallCard part={makePart({ status: "error" })} />);
    const statusEl = document.querySelector(".tool-status-error");
    expect(statusEl).toBeInTheDocument();
  });

  it("shows running indicator for running status", () => {
    render(<ToolCallCard part={makePart({ status: "running" })} />);
    expect(document.querySelector(".tool-status-running")).toBeInTheDocument();
  });

  it("shows running indicator for pending status", () => {
    render(<ToolCallCard part={makePart({ status: "pending" })} />);
    expect(document.querySelector(".tool-status-running")).toBeInTheDocument();
  });

  it("displays duration in seconds when start and end times present", () => {
    render(<ToolCallCard part={makePart({ startedAt: 1000, completedAt: 2500 })} />);
    expect(screen.getByText("1.5s")).toBeInTheDocument();
  });

  it("displays millisecond duration for sub-second times", () => {
    render(<ToolCallCard part={makePart({ startedAt: 1000, completedAt: 1150 })} />);
    expect(screen.getByText("150ms")).toBeInTheDocument();
  });

  it("hides duration when times are missing", () => {
    render(<ToolCallCard part={makePart({ startedAt: undefined, completedAt: undefined })} />);
    expect(screen.queryByText(/ms$/)).not.toBeInTheDocument();
    expect(screen.queryByText(/^\d+\.\ds$/)).not.toBeInTheDocument();
  });

  it("shows bash icon for bash tool", () => {
    render(<ToolCallCard part={makePart({ name: "bash" })} />);
    expect(screen.getByText("$")).toBeInTheDocument();
  });

  it("shows file icon for read tool", () => {
    render(<ToolCallCard part={makePart({ name: "read_file" })} />);
    expect(screen.getByText("#")).toBeInTheDocument();
  });

  it("shows search icon for grep tool", () => {
    render(<ToolCallCard part={makePart({ name: "grep" })} />);
    expect(screen.getByText("?")).toBeInTheDocument();
  });

  it("shows default icon for unknown tool", () => {
    render(<ToolCallCard part={makePart({ name: "custom_tool" })} />);
    expect(screen.getByText(">")).toBeInTheDocument();
  });

  it("collapses completed tool by default with defaultOpen=false", () => {
    render(<ToolCallCard part={makePart()} defaultOpen={false} />);
    expect(screen.queryByText("Input")).not.toBeInTheDocument();
  });

  it("expands on click to show input/output", async () => {
    render(<ToolCallCard part={makePart()} defaultOpen={false} />);
    const header = screen.getByRole("button", { name: /bash/ });
    await userEvent.click(header);
    expect(screen.getByText("Input")).toBeInTheDocument();
    expect(screen.getByText("Output")).toBeInTheDocument();
  });

  it("applies compact class for completed collapsed state", () => {
    render(<ToolCallCard part={makePart()} defaultOpen={false} />);
    expect(document.querySelector(".tool-call-compact")).toBeInTheDocument();
  });

  it("applies error class for error status", () => {
    render(<ToolCallCard part={makePart({ status: "error" })} />);
    expect(document.querySelector(".tool-call-error")).toBeInTheDocument();
  });

  it("sets aria-expanded attribute on header", async () => {
    render(<ToolCallCard part={makePart()} defaultOpen={false} />);
    const header = screen.getByRole("button", { name: /bash/ });
    expect(header).toHaveAttribute("aria-expanded", "false");

    await userEvent.click(header);
    expect(header).toHaveAttribute("aria-expanded", "true");
  });
});
