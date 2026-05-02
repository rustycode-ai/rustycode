import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, cleanup, act } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ToolApprovalModal } from "../ToolApprovalModal";
import type { ToolApprovalRequestPayload } from "../../protocol/types";

afterEach(cleanup);

function makeRequest(overrides: Partial<ToolApprovalRequestPayload> = {}): ToolApprovalRequestPayload {
  return {
    request_id: "req-1",
    tool_name: "bash",
    input_preview: "rm -rf /tmp/test",
    risk_level: "medium",
    ...overrides,
  };
}

describe("ToolApprovalModal", () => {
  it("renders nothing when request is null", () => {
    const { container } = render(<ToolApprovalModal request={null} onRespond={vi.fn()} />);
    expect(container.innerHTML).toBe("");
  });

  it("renders tool name", () => {
    render(<ToolApprovalModal request={makeRequest()} onRespond={vi.fn()} />);
    expect(screen.getByText("bash")).toBeInTheDocument();
  });

  it("renders input preview", () => {
    render(<ToolApprovalModal request={makeRequest()} onRespond={vi.fn()} />);
    expect(screen.getByText("rm -rf /tmp/test")).toBeInTheDocument();
  });

  it("renders risk level label", () => {
    render(<ToolApprovalModal request={makeRequest({ risk_level: "high" })} onRespond={vi.fn()} />);
    expect(screen.getByText("High Risk")).toBeInTheDocument();
  });

  it("renders low risk label", () => {
    render(<ToolApprovalModal request={makeRequest({ risk_level: "low" })} onRespond={vi.fn()} />);
    expect(screen.getByText("Low Risk")).toBeInTheDocument();
  });

  it("has dialog role and aria-modal", () => {
    render(<ToolApprovalModal request={makeRequest()} onRespond={vi.fn()} />);
    const dialog = screen.getByRole("dialog");
    expect(dialog).toHaveAttribute("aria-modal", "true");
    expect(dialog).toHaveAttribute("aria-label", "Tool approval request");
  });

  it("calls onRespond(true) when approve button clicked", async () => {
    const onRespond = vi.fn();
    render(<ToolApprovalModal request={makeRequest()} onRespond={onRespond} />);
    await userEvent.click(screen.getByRole("button", { name: /Approve bash/ }));
    expect(onRespond).toHaveBeenCalledWith("req-1", true);
  });

  it("calls onRespond(false) when deny button clicked", async () => {
    const onRespond = vi.fn();
    render(<ToolApprovalModal request={makeRequest()} onRespond={onRespond} />);
    await userEvent.click(screen.getByRole("button", { name: /Deny bash/ }));
    expect(onRespond).toHaveBeenCalledWith("req-1", false);
  });

  it("calls onRespond(false) when overlay backdrop clicked", async () => {
    const onRespond = vi.fn();
    render(<ToolApprovalModal request={makeRequest()} onRespond={onRespond} />);
    const overlay = screen.getByRole("dialog");
    await userEvent.click(overlay);
    expect(onRespond).toHaveBeenCalledWith("req-1", false);
  });

  it("calls onRespond(true) when Y key pressed", () => {
    const onRespond = vi.fn();
    render(<ToolApprovalModal request={makeRequest()} onRespond={onRespond} />);
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "y" }));
    expect(onRespond).toHaveBeenCalledWith("req-1", true);
  });

  it("calls onRespond(false) when N key pressed", () => {
    const onRespond = vi.fn();
    render(<ToolApprovalModal request={makeRequest()} onRespond={onRespond} />);
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "n" }));
    expect(onRespond).toHaveBeenCalledWith("req-1", false);
  });

  it("calls onRespond(false) when Escape key pressed", () => {
    const onRespond = vi.fn();
    render(<ToolApprovalModal request={makeRequest()} onRespond={onRespond} />);
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    expect(onRespond).toHaveBeenCalledWith("req-1", false);
  });

  it("does not respond to keyboard when request is null", () => {
    const onRespond = vi.fn();
    render(<ToolApprovalModal request={null} onRespond={onRespond} />);
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "y" }));
    expect(onRespond).not.toHaveBeenCalled();
  });

  it("renders elapsed timer starting at 0s", () => {
    vi.useFakeTimers();
    render(<ToolApprovalModal request={makeRequest()} onRespond={vi.fn()} />);
    expect(screen.getByText("0s")).toBeInTheDocument();
    vi.useRealTimers();
  });

  it("updates elapsed timer after time passes", () => {
    vi.useFakeTimers();
    render(<ToolApprovalModal request={makeRequest()} onRespond={vi.fn()} />);
    act(() => { vi.advanceTimersByTime(3500); });
    expect(screen.getByText("3s")).toBeInTheDocument();
    vi.useRealTimers();
  });
});
