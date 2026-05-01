interface SettingsPanelProps {
  provider: string;
  model: string;
  open: boolean;
  onClose: () => void;
}

export function SettingsPanel({ provider, model, open, onClose }: SettingsPanelProps) {
  if (!open) return null;

  return (
    <div className="settings-overlay" onClick={onClose} role="dialog" aria-label="Settings" aria-modal="true">
      <div className="settings-panel" onClick={(e) => e.stopPropagation()}>
        <div className="settings-header">
          <h2>Settings</h2>
          <button className="btn-icon" onClick={onClose} aria-label="Close settings">x</button>
        </div>
        <div className="settings-body">
          <section className="settings-section">
            <h3>Model</h3>
            <div className="settings-row">
              <span className="settings-label">Provider</span>
              <span className="settings-value">{provider || "unknown"}</span>
            </div>
            <div className="settings-row">
              <span className="settings-label">Model</span>
              <span className="settings-value settings-mono">{model || "unknown"}</span>
            </div>
          </section>
          <section className="settings-section">
            <h3>Keyboard Shortcuts</h3>
            <div className="settings-row">
              <span className="settings-label">Toggle sidebar</span>
              <kbd className="settings-kbd">Ctrl+B</kbd>
            </div>
            <div className="settings-row">
              <span className="settings-label">Toggle tool output</span>
              <kbd className="settings-kbd">Ctrl+/</kbd>
            </div>
            <div className="settings-row">
              <span className="settings-label">Send message</span>
              <kbd className="settings-kbd">Enter</kbd>
            </div>
            <div className="settings-row">
              <span className="settings-label">New line</span>
              <kbd className="settings-kbd">Shift+Enter</kbd>
            </div>
            <div className="settings-row">
              <span className="settings-label">Abort generation</span>
              <kbd className="settings-kbd">Escape</kbd>
            </div>
          </section>
          <section className="settings-section">
            <h3>Security</h3>
            <p className="settings-note">
              API keys are stored server-side and never exposed to the browser.
              Configure them via environment variables on the server.
            </p>
          </section>
        </div>
      </div>
    </div>
  );
}
