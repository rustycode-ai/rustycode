import { useRef, useEffect, useState, useCallback } from "react";
import type { FrontendMessage } from "../protocol/types";
import { MessageBubble } from "./MessageBubble";

interface MessageListProps {
  messages: FrontendMessage[];
  toolOutputsVisible: boolean;
  pending: boolean;
  scrollContainerRef?: React.RefObject<HTMLDivElement | null>;
}

export function MessageList({ messages, toolOutputsVisible, pending, scrollContainerRef }: MessageListProps) {
  const bottomRef = useRef<HTMLDivElement>(null);
  const ownRef = useRef<HTMLDivElement>(null);
  const [userScrolledUp, setUserScrolledUp] = useState(false);

  const container = scrollContainerRef?.current;

  const handleScroll = useCallback(() => {
    const el = container;
    if (!el) return;
    const distanceFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight;
    setUserScrolledUp(distanceFromBottom > 80);
  }, [container]);

  const scrollToBottom = useCallback(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
    setUserScrolledUp(false);
  }, []);

  useEffect(() => {
    if (!userScrolledUp) {
      bottomRef.current?.scrollIntoView({ behavior: "smooth" });
    }
  }, [messages, userScrolledUp]);

  useEffect(() => {
    const el = container;
    if (!el) return;
    el.addEventListener("scroll", handleScroll, { passive: true });
    return () => el.removeEventListener("scroll", handleScroll);
  }, [container, handleScroll]);

  const lastIndex = messages.length - 1;

  return (
    <div className="message-list" ref={ownRef} role="log" aria-label="Conversation messages" aria-live="polite">
      {messages.length === 0 ? (
        <div className="message-empty">
          <h2>RustyCode</h2>
          <p>Send a message to start a conversation.</p>
          <div className="empty-hints">
            <span className="empty-hint"><kbd>⌘K</kbd> Commands</span>
            <span className="empty-hint"><kbd>⌘B</kbd> Sessions</span>
            <span className="empty-hint"><kbd>⌘/</kbd> Tool output</span>
          </div>
        </div>
      ) : (
        messages.map((msg, i) => (
          <MessageBubble
            key={msg.id}
            message={msg}
            toolOutputsVisible={toolOutputsVisible}
            isStreaming={pending && i === lastIndex && msg.kind === "Assistant"}
          />
        ))
      )}
      <div ref={bottomRef} />
      {userScrolledUp && (
        <button
          className="scroll-to-bottom"
          onClick={scrollToBottom}
          type="button"
          aria-label="Scroll to latest messages"
        >
          ↓ Latest
        </button>
      )}
    </div>
  );
}
