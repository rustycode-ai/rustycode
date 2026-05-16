import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, cleanup } from "@testing-library/react";
import { MessageList } from "../MessageList";
import type { FrontendMessage } from "../../protocol/types";

// jsdom does not implement scrollIntoView
Element.prototype.scrollIntoView = vi.fn();

afterEach(cleanup);

function makeMessages(count: number, overrides?: Partial<FrontendMessage>): FrontendMessage[] {
  const now = Date.now();
  return Array.from({ length: count }, (_, i) => ({
    id: `m${i}`,
    content: `Message ${i}`,
    kind: (i % 2 === 0 ? "User" : "Assistant") as FrontendMessage["kind"],
    parts: [],
    created_at: now + i * 1000,
    ...overrides,
  }));
}

describe("MessageList", () => {
  it("shows empty state when no messages", () => {
    render(<MessageList messages={[]} toolOutputsVisible={false} pending={false} />);
    expect(screen.getByText("RustyCode")).toBeInTheDocument();
    expect(screen.getByText("Send a message to start a conversation.")).toBeInTheDocument();
  });

  it("renders messages", () => {
    render(<MessageList messages={makeMessages(3)} toolOutputsVisible={false} pending={false} />);
    expect(screen.getByText("Message 0")).toBeInTheDocument();
    expect(screen.getByText("Message 2")).toBeInTheDocument();
  });

  it("has role=log", () => {
    render(<MessageList messages={[]} toolOutputsVisible={false} pending={false} />);
    expect(screen.getByRole("log")).toBeInTheDocument();
  });

  it("has aria-live=polite", () => {
    render(<MessageList messages={[]} toolOutputsVisible={false} pending={false} />);
    expect(screen.getByRole("log")).toHaveAttribute("aria-live", "polite");
  });

  it("shows keyboard shortcuts in empty state", () => {
    render(<MessageList messages={[]} toolOutputsVisible={false} pending={false} />);
    expect(screen.getByText("Commands")).toBeInTheDocument();
    expect(screen.getByText("Sessions")).toBeInTheDocument();
    expect(screen.getByText("Shortcuts")).toBeInTheDocument();
  });

  it("does not show empty state when messages exist", () => {
    render(<MessageList messages={makeMessages(1)} toolOutputsVisible={false} pending={false} />);
    expect(screen.queryByText("RustyCode")).not.toBeInTheDocument();
  });

  it("groups messages by day with separator", () => {
    const now = Date.now();
    const yesterday = now - 86_400_000;
    const messages: FrontendMessage[] = [
      { id: "m1", content: "Yesterday msg", kind: "User", parts: [], created_at: yesterday },
      { id: "m2", content: "Today msg", kind: "Assistant", parts: [], created_at: now },
    ];
    render(<MessageList messages={messages} toolOutputsVisible={false} pending={false} />);
    expect(screen.getByText("Yesterday")).toBeInTheDocument();
    expect(screen.getByText("Today")).toBeInTheDocument();
  });

  it("does not show date separator for same-day messages", () => {
    const messages = makeMessages(3);
    render(<MessageList messages={messages} toolOutputsVisible={false} pending={false} />);
    // Only one separator ("Today") should appear, not multiple
    const separators = document.querySelectorAll(".date-separator");
    expect(separators.length).toBe(1);
  });

  it("renders date separator with role=presentation", () => {
    render(<MessageList messages={makeMessages(1)} toolOutputsVisible={false} pending={false} />);
    expect(screen.getByText("Today").closest("[role='presentation']")).toBeInTheDocument();
  });
});
