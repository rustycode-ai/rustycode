import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, cleanup } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ToastContainer } from "../ToastContainer";
import type { Toast } from "../../hooks/useToast";

afterEach(cleanup);

function makeToast(overrides: Partial<Toast> = {}): Toast {
  return { id: 1, message: "Saved", variant: "success", ...overrides };
}

describe("ToastContainer", () => {
  it("renders nothing when toasts array is empty", () => {
    const { container } = render(<ToastContainer toasts={[]} onDismiss={vi.fn()} />);
    expect(container.innerHTML).toBe("");
  });

  it("renders toast message", () => {
    render(<ToastContainer toasts={[makeToast()]} onDismiss={vi.fn()} />);
    expect(screen.getByText("Saved")).toBeInTheDocument();
  });

  it("renders multiple toasts", () => {
    render(
      <ToastContainer
        toasts={[makeToast({ id: 1, message: "First" }), makeToast({ id: 2, message: "Second" })]}
        onDismiss={vi.fn()}
      />
    );
    expect(screen.getByText("First")).toBeInTheDocument();
    expect(screen.getByText("Second")).toBeInTheDocument();
  });

  it("applies variant class", () => {
    render(<ToastContainer toasts={[makeToast({ variant: "error" })]} onDismiss={vi.fn()} />);
    expect(document.querySelector(".toast-error")).toBeInTheDocument();
  });

  it("applies success variant class", () => {
    render(<ToastContainer toasts={[makeToast({ variant: "success" })]} onDismiss={vi.fn()} />);
    expect(document.querySelector(".toast-success")).toBeInTheDocument();
  });

  it("applies info variant class", () => {
    render(<ToastContainer toasts={[makeToast({ variant: "info" })]} onDismiss={vi.fn()} />);
    expect(document.querySelector(".toast-info")).toBeInTheDocument();
  });

  it("calls onDismiss with toast id on click", async () => {
    const onDismiss = vi.fn();
    render(<ToastContainer toasts={[makeToast({ id: 42 })]} onDismiss={onDismiss} />);
    await userEvent.click(screen.getByText("Saved"));
    expect(onDismiss).toHaveBeenCalledWith(42);
  });

  it("has role=log on container", () => {
    render(<ToastContainer toasts={[makeToast()]} onDismiss={vi.fn()} />);
    expect(screen.getByRole("log")).toBeInTheDocument();
  });

  it("each toast has role=alert", () => {
    render(
      <ToastContainer
        toasts={[makeToast({ id: 1, message: "A" }), makeToast({ id: 2, message: "B" })]}
        onDismiss={vi.fn()}
      />
    );
    expect(screen.getAllByRole("alert")).toHaveLength(2);
  });
});
