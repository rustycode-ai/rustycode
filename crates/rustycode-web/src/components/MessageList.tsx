import { useRef, useEffect, useState, useCallback } from "react";
import type { FrontendMessage } from "../protocol/types";
import { MessageBubble } from "./MessageBubble";

interface MessageListProps {
  messages: FrontendMessage[];
  toolOutputsVisible: boolean;
  pending: boolean;
  scrollContainerRef?: React.RefObject<HTMLDivElement | null>;
}

function formatDayLabel(ts: number): string {
  const date = new Date(ts);
  const now = new Date();
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const msgDate = new Date(date.getFullYear(), date.getMonth(), date.getDate());
  const diffDays = Math.round((today.getTime() - msgDate.getTime()) / 86_400_000);
  if (diffDays === 0) return "Today";
  if (diffDays === 1) return "Yesterday";
  return date.toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" });
}

function getDayKey(ts: number | undefined): string {
  if (!ts) return "unknown";
  const d = new Date(ts);
  return `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`;
}

interface MessageOrSeparator {
  type: "message" | "separator";
  key: string;
  message?: FrontendMessage;
  index?: number;
  label?: string;
}

function groupByDay(messages: FrontendMessage[]): MessageOrSeparator[] {
  const result: MessageOrSeparator[] = [];
  let lastDayKey = "";
  for (let i = 0; i < messages.length; i++) {
    const msg = messages[i];
    const dayKey = getDayKey(msg.created_at);
    if (dayKey !== lastDayKey) {
      result.push({
        type: "separator",
        key: `sep-${dayKey}`,
        label: msg.created_at ? formatDayLabel(msg.created_at) : "History",
      });
      lastDayKey = dayKey;
    }
    result.push({ type: "message", key: msg.id, message: msg, index: i });
  }  return result;
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
  const items = groupByDay(messages);

  return (
    <div className="message-list" ref={ownRef} role="log" aria-label="Conversation messages" aria-live="polite">
      {messages.length === 0 ? (
        <div className="message-empty">
          <h2>RustyCode</h2>
          <p>Send a message to start a conversation.</p>
          <div className="empty-hints">
            <span className="empty-hint"><kbd>⌘K</kbd> Commands</span>
            <span className="empty-hint"><kbd>⌘B</kbd> Sessions</span>
            <span className="empty-hint"><kbd>⌘/</kbd> Shortcuts</span>
          </div>
        </div>
      ) : (
        items.map((item) =>
          item.type === "separator" ? (
            <div key={item.key} className="date-separator" role="presentation">
              <span className="date-separator-label">{item.label}</span>
            </div>
          ) : item.message ? (
            <MessageBubble
              key={item.key}
              message={item.message}
              toolOutputsVisible={toolOutputsVisible}
              isStreaming={pending && item.index === lastIndex && item.message.kind === "Assistant"}
            />
          ) : null
        )
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
