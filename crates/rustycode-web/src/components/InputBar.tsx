import { useState, useRef, useCallback, useEffect, type KeyboardEvent } from "react";

interface InputBarProps {
  onSend: (content: string) => void;
  onAbort: () => void;
  pending: boolean;
}

export function InputBar({ onSend, onAbort, pending }: InputBarProps) {
  const [value, setValue] = useState("");
  const sendingRef = useRef(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

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
    }
  };

  const handleSend = () => {
    const trimmed = value.trim();
    if (!trimmed || pending || sendingRef.current) return;
    sendingRef.current = true;
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
        {value.length > 0 && <span className="input-char-count">{value.length}</span>}
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
