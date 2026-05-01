import { useState, useRef, type KeyboardEvent } from "react";

interface InputBarProps {
  onSend: (content: string) => void;
  onAbort: () => void;
  pending: boolean;
}

export function InputBar({ onSend, onAbort, pending }: InputBarProps) {
  const [value, setValue] = useState("");
  const sendingRef = useRef(false);

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
    // Reset after React has re-rendered with pending=true
    requestAnimationFrame(() => { sendingRef.current = false; });
  };

  return (
    <div className="input-bar">
      <textarea
        value={value}
        onChange={(e) => setValue(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder={pending ? "Waiting for response..." : "Type a message..."}
        disabled={pending}
        rows={1}
      />
      {pending ? (
        <button className="btn-abort" onClick={onAbort} type="button">
          Stop
        </button>
      ) : (
        <button className="btn-send" onClick={handleSend} disabled={!value.trim()} type="button">
          Send
        </button>
      )}
    </div>
  );
}
