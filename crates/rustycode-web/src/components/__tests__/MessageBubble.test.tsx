import { describe, it, expect, afterEach } from "vitest";
import { render, screen, cleanup } from "@testing-library/react";
import { MessageBubble } from "../MessageBubble";
import type { FrontendMessage, MessagePart } from "../../protocol/types";

afterEach(cleanup);

function makeUserMessage(overrides?: Partial<FrontendMessage>): FrontendMessage {
  return {
    id: "m1",
    content: "Hello, how are you?",
    kind: "User",
    parts: [],
    created_at: Date.now(),
    ...overrides,
  };
}

function makeAssistantMessage(parts?: MessagePart[], overrides?: Partial<FrontendMessage>): FrontendMessage {
  return {
    id: "m2",
    content: "I am doing great!",
    kind: "Assistant",
    parts: parts ?? [],
    created_at: Date.now(),
    ...overrides,
  };
}

describe("MessageBubble", () => {
  it("renders user message content", () => {
    render(<MessageBubble message={makeUserMessage()} />);
    expect(screen.getByText("Hello, how are you?")).toBeInTheDocument();
  });

  it("applies user class for user messages", () => {
    render(<MessageBubble message={makeUserMessage()} />);
    expect(document.querySelector(".message-user")).toBeInTheDocument();
  });

  it("applies assistant class for assistant messages", () => {
    render(<MessageBubble message={makeAssistantMessage()} />);
    expect(document.querySelector(".message-assistant")).toBeInTheDocument();
  });

  it("renders assistant message with markdown content", () => {
    render(<MessageBubble message={makeAssistantMessage()} />);
    expect(screen.getByText("I am doing great!")).toBeInTheDocument();
  });

  it("sets data-message-id", () => {
    render(<MessageBubble message={makeUserMessage()} />);
    expect(document.querySelector("[data-message-id='m1']")).toBeInTheDocument();
  });

  it("sets role=article on assistant messages", () => {
    render(<MessageBubble message={makeAssistantMessage()} />);
    expect(screen.getByRole("article")).toBeInTheDocument();
  });

  it("does not set role=article on user messages", () => {
    render(<MessageBubble message={makeUserMessage()} />);
    expect(screen.queryByRole("article")).not.toBeInTheDocument();
  });

  it("renders timestamp as time element", () => {
    render(<MessageBubble message={makeUserMessage({ created_at: Date.now() })} />);
    expect(screen.getByText("just now")).toBeInTheDocument();
  });

  it("hides timestamp when created_at is undefined", () => {
    render(<MessageBubble message={makeUserMessage({ created_at: undefined })} />);
    expect(document.querySelector("time")).not.toBeInTheDocument();
  });

  it("shows copy button", () => {
    render(<MessageBubble message={makeUserMessage()} />);
    expect(screen.getByLabelText("Copy message")).toBeInTheDocument();
  });

  it("applies streaming class when isStreaming is true", () => {
    render(<MessageBubble message={makeAssistantMessage()} isStreaming />);
    expect(document.querySelector(".message-streaming")).toBeInTheDocument();
  });

  it("does not apply streaming class by default", () => {
    render(<MessageBubble message={makeAssistantMessage()} />);
    expect(document.querySelector(".message-streaming")).not.toBeInTheDocument();
  });

  it("renders streaming cursor when streaming with empty content", () => {
    render(<MessageBubble message={makeAssistantMessage([], { content: "" })} isStreaming />);
    expect(document.querySelector(".streaming-cursor")).toBeInTheDocument();
  });

  it("shows loading indicator when no content and not streaming", () => {
    render(<MessageBubble message={makeAssistantMessage([], { content: "" })} />);
    expect(screen.getByLabelText("Loading")).toBeInTheDocument();
  });

  it("renders text parts", () => {
    const parts: MessagePart[] = [{ type: "text", content: "Part text here" }];
    render(<MessageBubble message={makeAssistantMessage(parts, { content: "" })} />);
    expect(screen.getByText("Part text here")).toBeInTheDocument();
  });

  it("renders thinking parts", () => {
    const parts: MessagePart[] = [{ type: "thinking", content: "I should think about this" }];
    render(<MessageBubble message={makeAssistantMessage(parts, { content: "" })} />);
    expect(screen.getByText(/Thinking.*chars/)).toBeInTheDocument();
  });

  it("renders error parts with role=alert", () => {
    const parts: MessagePart[] = [{ type: "error", message: "Something went wrong" }];
    render(<MessageBubble message={makeAssistantMessage(parts, { content: "" })} />);
    expect(screen.getByRole("alert")).toHaveTextContent("Something went wrong");
  });

  it("renders aria-label on assistant message", () => {
    render(<MessageBubble message={makeAssistantMessage()} />);
    expect(screen.getByRole("article")).toHaveAttribute("aria-label", "Assistant message");
  });

  it("formats relative time as minutes ago", () => {
    const fiveMinAgo = Date.now() - 5 * 60_000;
    render(<MessageBubble message={makeUserMessage({ created_at: fiveMinAgo })} />);
    expect(screen.getByText("5m ago")).toBeInTheDocument();
  });
});
