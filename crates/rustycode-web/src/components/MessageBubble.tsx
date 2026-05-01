import Markdown from "react-markdown";
import type { FrontendMessage, MessagePart } from "../protocol/types";
import { ToolCallCard } from "./ToolCallCard";

const ALLOWED_ELEMENTS = [
  "p", "br", "code", "pre", "strong", "em", "ul", "ol", "li", "a",
  "h1", "h2", "h3", "h4", "blockquote", "hr", "table", "thead", "tbody",
  "tr", "th", "td", "span", "del", "sup", "sub",
];

interface MessageBubbleProps {
  message: FrontendMessage;
  toolOutputsVisible?: boolean;
}

function TextPartRenderer({ content }: { content: string }) {
  return (
    <div className="part-text">
      <Markdown allowedElements={ALLOWED_ELEMENTS}>{content}</Markdown>
    </div>
  );
}

function ThinkingPartRenderer({ content }: { content: string }) {
  return (
    <details className="part-thinking">
      <summary>Thinking...</summary>
      <pre>{content}</pre>
    </details>
  );
}

function ErrorPartRenderer({ message }: { message: string }) {
  return (
    <div className="part-error">
      <span className="part-error-icon">&#9888;</span>
      <span>{message}</span>
    </div>
  );
}

function PartRenderer({ part, toolOutputsVisible }: { part: MessagePart; toolOutputsVisible?: boolean }) {
  switch (part.type) {
    case "text":
      return <TextPartRenderer content={part.content} />;
    case "thinking":
      return <ThinkingPartRenderer content={part.content} />;
    case "tool_call":
      return <ToolCallCard part={part} defaultOpen={toolOutputsVisible} />;
    case "error":
      return <ErrorPartRenderer message={part.message} />;
  }
}

export function MessageBubble({ message, toolOutputsVisible }: MessageBubbleProps) {
  const className = `message-bubble message-${message.kind.toLowerCase()}`;

  // User and System messages may have parts or fall back to flat content
  if (message.kind === "User") {
    return (
      <div className={className}>
        <div className="message-content">
          <p>{message.content}</p>
        </div>
      </div>
    );
  }

  // Assistant messages use parts
  const hasParts = message.parts.length > 0;

  return (
    <div className={className}>
      {hasParts ? (
        <div className="message-parts">
          {message.parts.map((part, i) => (
            <PartRenderer key={i} part={part} toolOutputsVisible={toolOutputsVisible} />
          ))}
        </div>
      ) : (
        <div className="message-content">
          {message.content ? (
            <Markdown allowedElements={ALLOWED_ELEMENTS}>{message.content}</Markdown>
          ) : (
            <span className="message-loading">...</span>
          )}
        </div>
      )}
    </div>
  );
}
