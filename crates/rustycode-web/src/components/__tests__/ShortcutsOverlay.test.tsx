import { describe, it, expect, vi, afterEach } from "vitest";
import { render, screen, cleanup } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ShortcutsOverlay } from "../ShortcutsOverlay";

afterEach(cleanup);

describe("ShortcutsOverlay", () => {
  it("renders keyboard shortcuts heading", () => {
    render(<ShortcutsOverlay onClose={() => {}} />);
    expect(screen.getByText("Keyboard Shortcuts")).toBeInTheDocument();
  });

  it("calls onClose when clicking backdrop", async () => {
    const onClose = vi.fn();
    render(<ShortcutsOverlay onClose={onClose} />);
    const backdrop = screen.getByRole("dialog");
    await userEvent.click(backdrop);
    expect(onClose).toHaveBeenCalled();
  });

  it("does not call onClose when clicking panel content", async () => {
    const onClose = vi.fn();
    const { container } = render(<ShortcutsOverlay onClose={onClose} />);
    const panel = container.querySelector(".shortcuts-panel")!;
    await userEvent.click(panel);
    expect(onClose).not.toHaveBeenCalled();
  });

  it("has accessible dialog role", () => {
    render(<ShortcutsOverlay onClose={() => {}} />);
    expect(screen.getByRole("dialog")).toHaveAttribute("aria-label", "Keyboard shortcuts");
  });

  it("has close button", () => {
    render(<ShortcutsOverlay onClose={() => {}} />);
    expect(screen.getByLabelText("Close")).toBeInTheDocument();
  });

  it("renders kbd elements for shortcuts", () => {
    render(<ShortcutsOverlay onClose={() => {}} />);
    const kbdElements = document.querySelectorAll("kbd");
    expect(kbdElements.length).toBeGreaterThan(0);
  });

  it("renders shortcut labels", () => {
    render(<ShortcutsOverlay onClose={() => {}} />);
    expect(screen.getByText("Command palette")).toBeInTheDocument();
    expect(screen.getByText("Send message")).toBeInTheDocument();
    expect(screen.getByText("Search messages")).toBeInTheDocument();
  });
});
