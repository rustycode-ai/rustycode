import { useState, useCallback } from "react";
import type { ToolCallPart } from "../protocol/types";

interface ToolCallCardProps {
  part: ToolCallPart;
  defaultOpen?: boolean;
}

function StatusIcon({ status }: { status: ToolCallPart["status"] }) {
  switch (status) {
    case "pending":
    case "running":
      return <span className="tool-status tool-status-running" />;
    case "completed":
      return <span className="tool-status tool-status-done">&#10003;</span>;
    case "error":
      return <span className="tool-status tool-status-error">&#10007;</span>;
  }
}

function formatDuration(start?: number, end?: number): string | null {
  if (!start || !end) return null;
  const ms = end - start;
  if (ms < 1000) return `${ms}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}

function toolIcon(name: string): string {
  if (name.includes("bash")) return "$";
  if (name.includes("read") || name.includes("write") || name.includes("edit")) return "#";
  if (name.includes("grep") || name.includes("glob") || name.includes("search")) return "?";
  if (name.includes("web")) return "@";
  return ">";
}

function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);

  const handleCopy = useCallback(() => {
    navigator.clipboard.writeText(text).then(
      () => { setCopied(true); setTimeout(() => setCopied(false), 2000); },
      () => {},
    );
  }, [text]);

  return (
    <button
      className="tool-copy-btn"
      onClick={handleCopy}
      type="button"
      aria-label={copied ? "Copied" : "Copy to clipboard"}
    >
      {copied ? "✓" : "Copy"}
    </button>
  );
}

const FILE_PATH_RE = /([\w./-]+\.(?:rs|ts|tsx|js|jsx|py|toml|yaml|yml|json|md|css|html|sql|sh|go|java|c|h|cpp))(?::?\d*(?::?\d*)?)/g;

/** Split text into safe segments, marking file-path matches for highlighting. */
function splitByFilePaths(text: string): Array<{ text: string; highlight: boolean }> {
  const segments: Array<{ text: string; highlight: boolean }> = [];
  let last = 0;
  for (const m of text.matchAll(FILE_PATH_RE)) {
    const idx = m.index!;
    if (idx > last) {
      segments.push({ text: text.slice(last, idx), highlight: false });
    }
    segments.push({ text: m[0], highlight: true });
    last = idx + m[0].length;
  }
  if (last < text.length) {
    segments.push({ text: text.slice(last), highlight: false });
  }
  return segments.length ? segments : [{ text, highlight: false }];
}

function ToolOutput({ text, isError }: { text: string; isError?: boolean }) {
  const segments = splitByFilePaths(text);
  return (
    <pre className={`tool-call-output ${isError ? "tool-output-error" : ""}`}>
      {segments.map((seg, i) =>
        seg.highlight
          ? <span key={i} className="tool-file-path">{seg.text}</span>
          : seg.text,
      )}
    </pre>
  );
}

export function ToolCallCard({ part, defaultOpen = true }: ToolCallCardProps) {
  const [expanded, setExpanded] = useState(defaultOpen ? part.status !== "completed" : false);
  const duration = formatDuration(part.startedAt, part.completedAt);
  const isCompleted = part.status === "completed";
  const isCompact = isCompleted && !expanded;

  return (
    <div className={`tool-call-card tool-call-${part.status}${isCompact ? " tool-call-compact" : ""}`}>
      <button
        className="tool-call-header"
        onClick={() => setExpanded(!expanded)}
        aria-expanded={expanded}
        aria-label={`${part.name} tool call${expanded ? ", collapse" : ", expand"}`}
      >
        <span className="tool-call-icon">{toolIcon(part.name)}</span>
        <span className="tool-call-name">{part.name}</span>
        <StatusIcon status={part.status} />
        {duration && <span className="tool-call-duration">{duration}</span>}
        <span className="tool-call-chevron">{expanded ? "▼" : "▶"}</span>
      </button>

      {expanded && (part.input || part.output) && (
        <div className="tool-call-body">
          {part.input && (
            <details open={part.status !== "completed"}>
              <summary>Input <CopyButton text={part.input} /></summary>
              <ToolOutput text={part.input} />
            </details>
          )}
          {part.output && (
            <details open={part.status === "error" || !part.input}>
              <summary>Output <CopyButton text={part.output} /></summary>
              <ToolOutput text={part.output} isError={part.status === "error"} />
            </details>
          )}
        </div>
      )}
    </div>
  );
}
