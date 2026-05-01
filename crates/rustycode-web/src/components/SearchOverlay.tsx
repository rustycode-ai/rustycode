import { useState, useEffect, useRef, useCallback } from "react";
import type { FrontendMessage } from "../protocol/types";

interface SearchOverlayProps {
  messages: FrontendMessage[];
  onClose: () => void;
  onNavigate: (messageId: string) => void;
}

export function SearchOverlay({ messages, onClose, onNavigate }: SearchOverlayProps) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<{ id: string; snippet: string; kind: string }[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  useEffect(() => {
    if (!query.trim()) {
      setResults([]);
      setSelectedIndex(0);
      return;
    }
    const lower = query.toLowerCase();
    const matches: { id: string; snippet: string; kind: string }[] = [];
    for (const msg of messages) {
      const text = msg.content.toLowerCase();
      const idx = text.indexOf(lower);
      if (idx !== -1) {
        const start = Math.max(0, idx - 30);
        const end = Math.min(msg.content.length, idx + query.length + 30);
        const snippet = (start > 0 ? "…" : "") + msg.content.slice(start, end) + (end < msg.content.length ? "…" : "");
        matches.push({ id: msg.id, snippet, kind: msg.kind });
      }
    }
    setResults(matches);
    setSelectedIndex(0);
  }, [query, messages]);

  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelectedIndex((prev) => Math.min(prev + 1, results.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelectedIndex((prev) => Math.max(prev - 1, 0));
    } else if (e.key === "Enter" && results.length > 0) {
      e.preventDefault();
      const selected = results[selectedIndex];
      if (selected) {
        onNavigate(selected.id);
        onClose();
      }
    }
  }, [onClose, results, selectedIndex, onNavigate]);

  const handleNavigate = useCallback((id: string) => {
    onNavigate(id);
    onClose();
  }, [onNavigate, onClose]);

  return (
    <div className="search-overlay" onClick={onClose} role="dialog" aria-label="Search messages">
      <div className="search-panel" onClick={(e) => e.stopPropagation()}>
        <div className="search-header">
          <input
            ref={inputRef}
            className="search-input"
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Search messages…"
            aria-label="Search query"
          />
          <span className="search-count">
            {results.length > 0 ? `${selectedIndex + 1}/${results.length}` : query ? "No results" : ""}
          </span>
          <button className="btn-icon" onClick={onClose} type="button" aria-label="Close search">✕</button>
        </div>
        {results.length > 0 && (
          <div className="search-results">
            {results.map((r, i) => (
              <button
                key={r.id}
                className={`search-result${i === selectedIndex ? " selected" : ""}`}
                onClick={() => handleNavigate(r.id)}
                type="button"
              >
                <span className="search-result-kind">{r.kind}</span>
                <span className="search-result-snippet">{r.snippet}</span>
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
