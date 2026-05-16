import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, cleanup, fireEvent } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SearchOverlay } from "../SearchOverlay";
import type { FrontendMessage } from "../../protocol/types";

afterEach(cleanup);

function makeMessages(): FrontendMessage[] {
  return [
    { id: "m1", content: "Hello, how are you?", kind: "User", parts: [] },
    { id: "m2", content: "I am doing great, thanks!", kind: "Assistant", parts: [] },
    { id: "m3", content: "Write a Rust function to sort an array", kind: "User", parts: [] },
  ];
}

describe("SearchOverlay", () => {
  it("renders search input", () => {
    render(<SearchOverlay messages={makeMessages()} onClose={vi.fn()} onNavigate={vi.fn()} />);
    expect(screen.getByPlaceholderText("Search messages…")).toBeInTheDocument();
  });

  it("has dialog role", () => {
    render(<SearchOverlay messages={makeMessages()} onClose={vi.fn()} onNavigate={vi.fn()} />);
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });

  it("shows no results initially", () => {
    render(<SearchOverlay messages={makeMessages()} onClose={vi.fn()} onNavigate={vi.fn()} />);
    expect(screen.queryByRole("button", { name: /how/ })).not.toBeInTheDocument();
  });

  it("finds matching messages", async () => {
    render(<SearchOverlay messages={makeMessages()} onClose={vi.fn()} onNavigate={vi.fn()} />);
    await userEvent.type(screen.getByPlaceholderText("Search messages…"), "hello");
    expect(screen.getByText(/Hello/)).toBeInTheDocument();
  });

  it("shows result count", async () => {
    render(<SearchOverlay messages={makeMessages()} onClose={vi.fn()} onNavigate={vi.fn()} />);
    await userEvent.type(screen.getByPlaceholderText("Search messages…"), "rust");
    expect(screen.getByText("1/1")).toBeInTheDocument();
  });

  it("shows no results for unmatched query", async () => {
    render(<SearchOverlay messages={makeMessages()} onClose={vi.fn()} onNavigate={vi.fn()} />);
    await userEvent.type(screen.getByPlaceholderText("Search messages…"), "xyz123");
    expect(screen.getByText("No results")).toBeInTheDocument();
  });

  it("calls onClose when backdrop clicked", async () => {
    const onClose = vi.fn();
    render(<SearchOverlay messages={makeMessages()} onClose={onClose} onNavigate={vi.fn()} />);
    await userEvent.click(screen.getByRole("dialog"));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("calls onClose when close button clicked", async () => {
    const onClose = vi.fn();
    render(<SearchOverlay messages={makeMessages()} onClose={onClose} onNavigate={vi.fn()} />);
    await userEvent.click(screen.getByRole("button", { name: "Close search" }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("calls onClose on Escape key", () => {
    const onClose = vi.fn();
    render(<SearchOverlay messages={makeMessages()} onClose={onClose} onNavigate={vi.fn()} />);
    fireEvent.keyDown(screen.getByPlaceholderText("Search messages…"), { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("calls onNavigate and onClose when result clicked", async () => {
    const onNavigate = vi.fn();
    const onClose = vi.fn();
    render(<SearchOverlay messages={makeMessages()} onClose={onClose} onNavigate={onNavigate} />);
    await userEvent.type(screen.getByPlaceholderText("Search messages…"), "rust");
    const result = screen.getByRole("button", { name: /Rust/ });
    await userEvent.click(result);
    expect(onNavigate).toHaveBeenCalledWith("m3");
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("shows message kind in results", async () => {
    render(<SearchOverlay messages={makeMessages()} onClose={vi.fn()} onNavigate={vi.fn()} />);
    await userEvent.type(screen.getByPlaceholderText("Search messages…"), "hello");
    expect(screen.getByText("User")).toBeInTheDocument();
  });
});
