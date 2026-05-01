import { useState } from "react";
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

export function ToolCallCard({ part, defaultOpen = true }: ToolCallCardProps) {
  const [expanded, setExpanded] = useState(defaultOpen ? part.status !== "completed" : false);
  const duration = formatDuration(part.startedAt, part.completedAt);

  return (
    <div className={`tool-call-card tool-call-${part.status}`}>
      <button
        className="tool-call-header"
        onClick={() => setExpanded(!expanded)}
        aria-expanded={expanded}
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
              <summary>Input</summary>
              <pre className="tool-call-output">{part.input}</pre>
            </details>
          )}
          {part.output && (
            <details open={part.status === "error" || !part.input}>
              <summary>Output</summary>
              <pre className={`tool-call-output ${part.status === "error" ? "tool-output-error" : ""}`}>
                {part.output}
              </pre>
            </details>
          )}
        </div>
      )}
    </div>
  );
}
