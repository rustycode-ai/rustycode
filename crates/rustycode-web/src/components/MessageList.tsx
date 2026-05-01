import { useRef, useEffect } from "react";
import type { FrontendMessage } from "../protocol/types";
import { MessageBubble } from "./MessageBubble";

interface MessageListProps {
  messages: FrontendMessage[];
  toolOutputsVisible: boolean;
}

export function MessageList({ messages, toolOutputsVisible }: MessageListProps) {
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  return (
    <div className="message-list">
      {messages.length === 0 ? (
        <div className="message-empty">
          <h2>RustyCode</h2>
          <p>Send a message to start a conversation.</p>
        </div>
      ) : (
        messages.map((msg) => (
          <MessageBubble key={msg.id} message={msg} toolOutputsVisible={toolOutputsVisible} />
        ))
      )}
      <div ref={bottomRef} />
    </div>
  );
}
