import { useEffect } from "react";

const SHORTCUTS = [
  { keys: ["⌘", "K"], label: "Command palette" },
  { keys: ["⌘", "B"], label: "Toggle sidebar" },
  { keys: ["⌘", "F"], label: "Search messages" },
  { keys: ["⌘", "/"], label: "Keyboard shortcuts" },
  { keys: ["Enter"], label: "Send message" },
  { keys: ["⇧", "Enter"], label: "New line in input" },
  { keys: ["Esc"], label: "Close dialog / Stop generation" },
  { keys: ["↑"], label: "Previous input history" },
  { keys: ["↓"], label: "Next input history" },
  { keys: ["↻"], label: "Regenerate last response" },
] as const;

interface ShortcutsOverlayProps {
  onClose: () => void;
}

export function ShortcutsOverlay({ onClose }: ShortcutsOverlayProps) {
  useEffect(() => {
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    };
    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  }, [onClose]);

  return (
    <div className="shortcuts-overlay" onClick={onClose} role="dialog" aria-label="Keyboard shortcuts">
      <div className="shortcuts-panel" onClick={(e) => e.stopPropagation()}>
        <div className="shortcuts-header">
          <h2>Keyboard Shortcuts</h2>
          <button className="btn-icon" onClick={onClose} type="button" aria-label="Close">✕</button>
        </div>
        <div className="shortcuts-body">
          {SHORTCUTS.map((s) => (
            <div key={s.label} className="shortcut-row">
              <span className="shortcut-label">{s.label}</span>
              <span className="shortcut-keys">
                {s.keys.map((k) => (
                  <kbd key={k} className="shortcut-key">{k}</kbd>
                ))}
              </span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
