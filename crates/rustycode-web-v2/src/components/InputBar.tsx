import { useState, type KeyboardEvent } from "react";

interface InputBarProps {
  onSend: (content: string) => void;
  onAbort: () => void;
  pending: boolean;
}

export function InputBar({ onSend, onAbort, pending }: InputBarProps) {
  const [value, setValue] = useState("");

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  const handleSend = () => {
    const trimmed = value.trim();
    if (!trimmed || pending) return;
    onSend(trimmed);
    setValue("");
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
