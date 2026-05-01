import Markdown from "react-markdown";
import type { FrontendMessage } from "../protocol/types";

interface MessageBubbleProps {
  message: FrontendMessage;
}

export function MessageBubble({ message }: MessageBubbleProps) {
  const className = `message-bubble message-${message.kind.toLowerCase()}`;

  return (
    <div className={className}>
      <div className="message-label">{message.kind}</div>
      <div className="message-content">
        {message.kind === "Assistant" || message.kind === "System" ? (
          <Markdown>{message.content}</Markdown>
        ) : message.kind === "Tool" || message.kind === "Error" ? (
          <pre className="tool-output">{message.content}</pre>
        ) : (
          <p>{message.content}</p>
        )}
      </div>
    </div>
  );
}
