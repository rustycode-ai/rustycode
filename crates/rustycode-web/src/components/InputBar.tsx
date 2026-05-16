import { useState, useRef, useCallback, useEffect, type KeyboardEvent } from "react";

interface InputBarProps {
  onSend: (content: string) => void;
  onAbort: () => void;
  pending: boolean;
  onRegenerate?: () => void;
}

const MAX_HISTORY = 100;

export function InputBar({ onSend, onAbort, pending, onRegenerate }: InputBarProps) {
  const [value, setValue] = useState("");
  const sendingRef = useRef(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const historyRef = useRef<string[]>([]);
  const historyIndexRef = useRef(-1);
  const draftRef = useRef("");

  useEffect(() => { textareaRef.current?.focus(); }, []);

  const autoResize = useCallback(() => {
    const el = textareaRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 200)}px`;
  }, []);

  useEffect(() => { autoResize(); }, [value, autoResize]);

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    } else if (e.key === "Escape" && pending) {
      e.preventDefault();
      onAbort();
    } else if (e.key === "ArrowUp" && !e.shiftKey) {
      handleHistoryUp(e);
    } else if (e.key === "ArrowDown" && !e.shiftKey) {
      handleHistoryDown(e);
    }
  };

  const handleHistoryUp = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    const textarea = e.currentTarget;
    if (textarea.selectionStart > 0) return;
    const history = historyRef.current;
    if (history.length === 0) return;
    e.preventDefault();
    if (historyIndexRef.current === -1) {
      draftRef.current = value;
      historyIndexRef.current = history.length - 1;
    } else if (historyIndexRef.current > 0) {
      historyIndexRef.current -= 1;
    }
    const entry = history[historyIndexRef.current];
    if (entry !== undefined) setValue(entry);
  };

  const handleHistoryDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    const textarea = e.currentTarget;
    if (textarea.selectionStart < value.length) return;
    if (historyIndexRef.current === -1) return;
    e.preventDefault();
    const history = historyRef.current;
    if (historyIndexRef.current >= history.length - 1) {
      historyIndexRef.current = -1;
      setValue(draftRef.current);
    } else {
      historyIndexRef.current += 1;
      const entry = history[historyIndexRef.current];
      if (entry !== undefined) setValue(entry);
    }
  };

  const handleSend = () => {
    const trimmed = value.trim();
    if (!trimmed || pending || sendingRef.current) return;
    sendingRef.current = true;
    const history = historyRef.current;
    if (history[history.length - 1] !== trimmed) {
      history.push(trimmed);
      if (history.length > MAX_HISTORY) history.shift();
    }
    historyIndexRef.current = -1;
    draftRef.current = "";
    onSend(trimmed);
    setValue("");
    requestAnimationFrame(() => { sendingRef.current = false; });
  };

  return (
    <div className="input-bar">
      <textarea
        ref={textareaRef}
        value={value}
        onChange={(e) => setValue(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder={pending ? "Response in progress — type ahead…" : "Message RustyCode… (Enter to send, Shift+Enter for newline)"}
        rows={1}
        aria-label="Message input"
      />
      <div className="input-bar-actions">
        {value.length > 0 && <span className="input-char-count" aria-label={`${value.length} characters`}>{value.length}</span>}
        {!pending && onRegenerate && (
          <button className="btn-icon btn-regenerate" onClick={onRegenerate} type="button" aria-label="Regenerate last response" title="Regenerate">
            ↻
          </button>
        )}
        {pending ? (
          <button className="btn-abort" onClick={onAbort} type="button" aria-label="Stop generation">
            Stop
          </button>
        ) : (
          <button className="btn-send" onClick={handleSend} disabled={!value.trim()} type="button" aria-label="Send message">
            Send
          </button>
        )}
      </div>
    </div>
  );
}
