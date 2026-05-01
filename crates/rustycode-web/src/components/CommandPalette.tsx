import { useState, useEffect, useCallback, useRef } from "react";
import type { FrontendMessage } from "../protocol/types";

interface SkillEntry {
  id: string;
  name: string;
  description: string;
  categories: string[];
}

interface ActionEntry {
  id: string;
  label: string;
  description: string;
  section: string;
  execute: () => void;
}

type PaletteItem = SkillItem | ActionItem;

interface SkillItem {
  type: "skill";
  id: string;
  label: string;
  description: string;
  section: string;
}

interface ActionItem {
  type: "action";
  id: string;
  label: string;
  description: string;
  section: string;
  execute: () => void;
}

interface CommandPaletteProps {
  open: boolean;
  onClose: () => void;
  sessionToken: string;
  messages: FrontendMessage[];
  onSkillExecuted?: (skillId: string) => void;
  onToggleSidebar?: () => void;
  onToggleToolOutputs?: () => void;
  onOpenModelSelector?: () => void;
}

function fuzzyMatch(query: string, text: string): boolean {
  const q = query.toLowerCase();
  const t = text.toLowerCase();
  if (t.includes(q)) return true;
  let qi = 0;
  for (let ti = 0; ti < t.length && qi < q.length; ti++) {
    if (t[ti] === q[qi]) qi++;
  }
  return qi === q.length;
}

function exportConversation(messages: FrontendMessage[]) {
  const lines: string[] = ["# RustyCode Conversation", ""];
  for (const msg of messages) {
    const role = msg.kind === "User" ? "**You**" : msg.kind === "Assistant" ? "**Assistant**" : `**${msg.kind}**`;
    const time = msg.created_at ? ` — ${new Date(msg.created_at).toLocaleString()}` : "";
    lines.push(`### ${role}${time}`, "");
    if (msg.parts.length > 0) {
      for (const part of msg.parts) {
        if (part.type === "text") {
          lines.push(part.content, "");
        } else if (part.type === "tool_call") {
          lines.push(`> **Tool: ${part.name}**${part.output ? "\n>\n> ```\n" + part.output.slice(0, 500) + "\n> ```" : ""}`, "");
        } else if (part.type === "thinking") {
          lines.push(`<details><summary>Thinking</summary>\n\n${part.content}\n\n</details>`, "");
        }
      }
    } else {
      lines.push(msg.content, "");
    }
    lines.push("---", "");
  }
  const blob = new Blob([lines.join("\n")], { type: "text/markdown" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = `rustycode-${new Date().toISOString().slice(0, 10)}.md`;
  a.click();
  URL.revokeObjectURL(url);
}

export function CommandPalette({
  open,
  onClose,
  sessionToken,
  messages,
  onSkillExecuted,
  onToggleSidebar,
  onToggleToolOutputs,
  onOpenModelSelector,
}: CommandPaletteProps) {
  const [skills, setSkills] = useState<SkillEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [search, setSearch] = useState("");
  const [highlightIdx, setHighlightIdx] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (open) {
      setLoading(true);
      fetch("/api/skills")
        .then((r) => (r.ok ? r.json() : { skills: [] }))
        .then((d) => setSkills(d.skills ?? []))
        .catch(() => {})
        .finally(() => setLoading(false));
      setSearch("");
      setHighlightIdx(0);
      setTimeout(() => inputRef.current?.focus(), 50);
    }
  }, [open]);

  const actions: ActionEntry[] = [
    {
      id: "action:toggle-sidebar",
      label: "Toggle Sidebar",
      description: "Show or hide the session sidebar",
      section: "Actions",
      execute: () => onToggleSidebar?.(),
    },
    {
      id: "action:toggle-tool-outputs",
      label: "Toggle Tool Outputs",
      description: "Show or hide tool call details",
      section: "Actions",
      execute: () => onToggleToolOutputs?.(),
    },
    {
      id: "action:switch-model",
      label: "Switch Model",
      description: "Open the model selector",
      section: "Actions",
      execute: () => onOpenModelSelector?.(),
    },
    {
      id: "action:export-conversation",
      label: "Export Conversation",
      description: "Download conversation as Markdown",
      section: "Actions",
      execute: () => exportConversation(messages),
    },
  ];

  const items: PaletteItem[] = [
    ...actions.map((a) => ({
      type: "action" as const,
      ...a,
    })),
    ...skills.map((s) => ({
      type: "skill" as const,
      id: s.id,
      label: s.name,
      description: s.description,
      section: s.categories[0] || "Skills",
    })),
  ];

  const filtered = search
    ? items.filter(
        (item) =>
          fuzzyMatch(search, item.label) ||
          fuzzyMatch(search, item.description) ||
          fuzzyMatch(search, item.id)
      )
    : items;

  useEffect(() => {
    setHighlightIdx(0);
  }, [search]);

  useEffect(() => {
    if (open && listRef.current) {
      const highlighted = listRef.current.querySelector("[data-highlighted='true']");
      highlighted?.scrollIntoView({ block: "nearest" });
    }
  }, [open, highlightIdx]);

  const executeSkill = useCallback(
    async (skillId: string) => {
      try {
        const res = await fetch("/api/skills/execute", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({
            skill_id: skillId,
            session_token: sessionToken,
          }),
        });
        if (res.ok) {
          onSkillExecuted?.(skillId);
        }
      } catch {
        // ignore
      }
      onClose();
    },
    [sessionToken, onSkillExecuted, onClose]
  );

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      } else if (e.key === "ArrowDown") {
        e.preventDefault();
        setHighlightIdx((i) => Math.min(i + 1, filtered.length - 1));
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setHighlightIdx((i) => Math.max(i - 1, 0));
      } else if (e.key === "Enter" && filtered[highlightIdx]) {
        e.preventDefault();
        const item = filtered[highlightIdx];
        if (item.type === "action") {
          (item as ActionItem).execute();
          onClose();
        } else {
          executeSkill(item.id);
        }
      }
    },
    [filtered, highlightIdx, onClose, executeSkill]
  );

  if (!open) return null;

  // Group by section
  const grouped: Record<string, PaletteItem[]> = {};
  for (const item of filtered) {
    const section = item.section;
    if (!grouped[section]) grouped[section] = [];
    grouped[section]!.push(item);
  }

  return (
    <div
      className="palette-overlay"
      onClick={onClose}
      role="dialog"
      aria-label="Command palette"
      aria-modal="true"
    >
      <div
        className="palette-dialog"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={handleKeyDown}
      >
        <div className="palette-header">
          <input
            ref={inputRef}
            className="palette-search"
            type="text"
            placeholder="Search commands and skills..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            aria-label="Search commands"
          />
        </div>
        <div className="palette-list" ref={listRef}>
          {loading ? (
            <div className="palette-loading" aria-hidden="true">
              <div className="skeleton skeleton-text" />
              <div className="skeleton skeleton-text" />
              <div className="skeleton skeleton-text skeleton-text-sm" />
              <div className="skeleton skeleton-text" />
            </div>
          ) : Object.entries(grouped).map(([section, sectionItems]) => (
            <div key={section} className="palette-section">
              <div className="palette-section-header">{section}</div>
              {sectionItems.map((item) => {
                const flatIdx = filtered.indexOf(item);
                return (
                  <button
                    key={item.id}
                    className={`palette-item ${flatIdx === highlightIdx ? "palette-item-highlight" : ""}`}
                    data-highlighted={flatIdx === highlightIdx}
                    onClick={() => {
                      if (item.type === "action") {
                        (item as ActionItem).execute();
                        onClose();
                      } else {
                        executeSkill(item.id);
                      }
                    }}
                    onMouseEnter={() => setHighlightIdx(flatIdx)}
                  >
                    <span className="palette-item-label">{item.label}</span>
                    <span className="palette-item-desc">{item.description}</span>
                    {item.type === "skill" && (
                      <span className="palette-item-badge">skill</span>
                    )}
                  </button>
                );
              })}
            </div>
          ))}
          {filtered.length === 0 && (
            <div className="palette-empty">No results found.</div>
          )}
        </div>
      </div>
    </div>
  );
}
