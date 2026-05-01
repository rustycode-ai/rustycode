import { useState, useCallback, type ReactNode } from "react";
import Markdown from "react-markdown";
import rehypeHighlight from "rehype-highlight";
import "highlight.js/styles/github-dark-dimmed.css";
import type { FrontendMessage, MessagePart } from "../protocol/types";
import { ToolCallCard } from "./ToolCallCard";

const ALLOWED_ELEMENTS = [
  "p", "br", "code", "pre", "strong", "em", "ul", "ol", "li", "a",
  "h1", "h2", "h3", "h4", "blockquote", "hr", "table", "thead", "tbody",
  "tr", "th", "td", "span", "del", "sup", "sub",
];

interface CodeBlockProps {
  className?: string;
  children?: ReactNode;
}

function extractText(children: ReactNode): string {
  if (typeof children === "string") return children;
  if (typeof children === "number") return String(children);
  if (Array.isArray(children)) return children.map(extractText).join("");
  if (children && typeof children === "object" && "props" in children) {
    return extractText((children as { props: { children?: ReactNode } }).props.children);
  }
  return "";
}

function CodeBlock({ className, children }: CodeBlockProps) {
  const [copied, setCopied] = useState(false);
  const lang = className?.replace("language-", "") || "";
  const codeText = extractText(children).replace(/\n$/, "");

  const handleCopy = useCallback(() => {
    navigator.clipboard.writeText(codeText).then(
      () => { setCopied(true); setTimeout(() => setCopied(false), 2000); },
      () => {},
    );
  }, [codeText]);

  return (
    <div className="code-block-wrapper">
      {lang && <span className="code-block-lang">{lang}</span>}
      <button
        className="code-block-copy"
        onClick={handleCopy}
        type="button"
        aria-label="Copy code"
      >
        {copied ? "✓" : "Copy"}
      </button>
      <pre><code className={className}>{children}</code></pre>
    </div>
  );
}

const markdownComponents = {
  pre: ({ children }: { children?: ReactNode }) => <>{children}</>,
  code: ({ className, children }: CodeBlockProps) => {
    const isInline = !className && typeof children === "string" && !children.includes("\n");
    return isInline
      ? <code className="inline-code">{children}</code>
      : <CodeBlock className={className}>{children}</CodeBlock>;
  },
};

interface MessageBubbleProps {
  message: FrontendMessage;
  toolOutputsVisible?: boolean;
  isStreaming?: boolean;
}

function formatTimestamp(ts?: number): string {
  if (!ts) return "";
  const diff = Date.now() - ts;
  if (diff < 60_000) return "just now";
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}m ago`;
  return new Date(ts).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

function timestampISO(ts?: number): string {
  if (!ts) return "";
  return new Date(ts).toISOString();
}

function TextPartRenderer({ content, streaming }: { content: string; streaming?: boolean }) {
  return (
    <div className={`part-text${streaming ? " part-text-streaming" : ""}`}>
      <Markdown
        allowedElements={ALLOWED_ELEMENTS}
        rehypePlugins={[rehypeHighlight]}
        components={markdownComponents}
      >{content}</Markdown>
    </div>
  );
}

function ThinkingPartRenderer({ content }: { content: string }) {
  const hasContent = content.trim().length > 0;
  return (
    <details className="part-thinking">
      <summary className="thinking-summary">
        <span className="thinking-icon" aria-hidden="true">◈</span>
        <span>Thinking{hasContent ? ` (${content.length.toLocaleString()} chars)` : "…"}</span>
      </summary>
      {hasContent && <pre className="thinking-content">{content}</pre>}
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

function PartRenderer({ part, toolOutputsVisible, streaming }: { part: MessagePart; toolOutputsVisible?: boolean; streaming?: boolean }) {
  switch (part.type) {
    case "text":
      return <TextPartRenderer content={part.content} streaming={streaming} />;
    case "thinking":
      return <ThinkingPartRenderer content={part.content} />;
    case "tool_call":
      return <ToolCallCard part={part} defaultOpen={toolOutputsVisible} />;
    case "error":
      return <ErrorPartRenderer message={part.message} />;
  }
}

function CopyMessageButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);
  const handleClick = useCallback(() => {
    navigator.clipboard.writeText(text).then(
      () => { setCopied(true); setTimeout(() => setCopied(false), 2000); },
      () => {},
    );
  }, [text]);
  return (
    <button
      className="message-action-btn"
      onClick={handleClick}
      type="button"
      aria-label="Copy message"
      data-copied={copied}
      title={copied ? "Copied" : "Copy message"}
    >
      {copied ? "✓" : "⧉"}
    </button>
  );
}

export function MessageBubble({ message, toolOutputsVisible, isStreaming }: MessageBubbleProps) {
  const className = `message-bubble message-${message.kind.toLowerCase()}${isStreaming ? " message-streaming" : ""}`;

  const timestamp = formatTimestamp(message.created_at);

  if (message.kind === "User") {
    return (
      <div className={className}>
        <div className="message-actions">
          <CopyMessageButton text={message.content} />
        </div>
        <div className="message-content">
          <p>{message.content}</p>
        </div>
        {timestamp && <time className="message-time" dateTime={timestampISO(message.created_at)}>{timestamp}</time>}
      </div>
    );
  }

  const hasParts = message.parts.length > 0;
  const copyText = hasParts
    ? message.parts.filter(p => p.type === "text").map(p => p.content).join("\n")
    : message.content;

  return (
    <div className={className} role="article" aria-label={`${message.kind} message`}>
      <div className="message-actions">
        <CopyMessageButton text={copyText} />
      </div>
      {hasParts ? (
        <div className="message-parts">
          {message.parts.map((part, i) => {
            const isLastText = isStreaming && part.type === "text" && i === message.parts.length - 1;
            return (
              <PartRenderer
                key={i}
                part={part}
                toolOutputsVisible={toolOutputsVisible}
                streaming={isLastText}
              />
            );
          })}
          {isStreaming && <span className="streaming-cursor" aria-hidden="true" />}
        </div>
      ) : (
        <div className="message-content">
          {message.content ? (
            <Markdown
              allowedElements={ALLOWED_ELEMENTS}
              rehypePlugins={[rehypeHighlight]}
              components={markdownComponents}
            >{message.content}</Markdown>
          ) : isStreaming ? (
            <span className="streaming-cursor" aria-hidden="true" />
          ) : (
            <span className="message-loading">…</span>
          )}
        </div>
      )}
      {timestamp && <time className="message-time" dateTime={timestampISO(message.created_at)}>{timestamp}</time>}
    </div>
  );
}
