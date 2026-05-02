import { useState, useEffect, useCallback, useMemo } from "react";

interface McpServer {
  name: string;
  command: string;
  args: string[];
  status: string;
}

interface SettingsPanelProps {
  provider: string;
  model: string;
  open: boolean;
  onClose: () => void;
}

export function SettingsPanel({ provider, model, open, onClose }: SettingsPanelProps) {
  const [servers, setServers] = useState<McpServer[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(false);
  const [adding, setAdding] = useState(false);
  const [newName, setNewName] = useState("");
  const [newCommand, setNewCommand] = useState("");
  const [newArgs, setNewArgs] = useState("");
  const [mutating, setMutating] = useState<Set<string>>(new Set());

  const platformKeys = useMemo(() => {
    const isMac = typeof navigator !== "undefined" && /Mac|iPod|iPhone|iPad/.test(navigator.platform);
    return {
      mod: isMac ? "⌘" : "Ctrl",
      shift: isMac ? "⇧" : "Shift",
    };
  }, []);

  const refreshServers = useCallback(async () => {
    setLoading(true);
    setError(false);
    try {
      const res = await fetch("/api/mcp/servers");
      if (res.ok) {
        const d: { servers: McpServer[] } = await res.json();
        setServers(d.servers);
      } else {
        setError(true);
      }
    } catch {
      setError(true);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (open) refreshServers();
  }, [open, refreshServers]);

  const handleAdd = useCallback(async () => {
    if (!newName.trim() || !newCommand.trim()) return;
    setMutating((prev) => new Set(prev).add("__add__"));
    try {
      const res = await fetch("/api/mcp/servers/add", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          name: newName.trim(),
          command: newCommand.trim(),
          args: newArgs.trim() ? newArgs.trim().split(/\s+/) : [],
        }),
      });
      if (!res.ok) {
        setError(true);
        return;
      }
      setNewName("");
      setNewCommand("");
      setNewArgs("");
      setAdding(false);
      await refreshServers();
    } catch {
      setError(true);
    } finally {
      setMutating((prev) => {
        const next = new Set(prev);
        next.delete("__add__");
        return next;
      });
    }
  }, [newName, newCommand, newArgs, refreshServers]);

  const handleRemove = useCallback(async (name: string) => {
    if (mutating.has(name)) return;
    setMutating((prev) => new Set(prev).add(name));
    try {
      const res = await fetch(`/api/mcp/servers/${encodeURIComponent(name)}`, { method: "DELETE" });
      if (!res.ok) {
        setError(true);
        return;
      }
      await refreshServers();
    } catch {
      setError(true);
    } finally {
      setMutating((prev) => {
        const next = new Set(prev);
        next.delete(name);
        return next;
      });
    }
  }, [refreshServers, mutating]);

  const handleRestart = useCallback(async (name: string) => {
    if (mutating.has(name)) return;
    setMutating((prev) => new Set(prev).add(name));
    try {
      const res = await fetch(`/api/mcp/servers/${encodeURIComponent(name)}/restart`, { method: "POST" });
      if (!res.ok) {
        setError(true);
        return;
      }
      await refreshServers();
    } catch {
      setError(true);
    } finally {
      setMutating((prev) => {
        const next = new Set(prev);
        next.delete(name);
        return next;
      });
    }
  }, [refreshServers, mutating]);

  if (!open) return null;

  return (
    <div className="settings-overlay" onClick={onClose} role="dialog" aria-labelledby="settings-title" aria-modal="true">
      <div className="settings-panel" onClick={(e) => e.stopPropagation()}>
        <div className="settings-header">
          <h2 id="settings-title">Settings</h2>
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
            <h3>MCP Servers</h3>
            {loading ? (
              <div aria-hidden="true">
                <div className="skeleton skeleton-text" />
                <div className="skeleton skeleton-text" />
                <div className="skeleton skeleton-text skeleton-text-sm" />
              </div>
            ) : error ? (
              <div className="settings-error">
                Failed to load MCP servers
                <button className="sidebar-retry" onClick={refreshServers} type="button">Retry</button>
              </div>
            ) : servers.length === 0 && !adding ? (
              <p className="settings-note">No MCP servers configured.</p>
            ) : null}
            {!loading && (
            <>
            <div className="mcp-server-list">
              {servers.map((s) => (
                <div key={s.name} className="mcp-server-row">
                  <div className="mcp-server-info">
                    <span className={`mcp-status-dot mcp-status-${s.status}`} />
                    <span className="mcp-server-name">{s.name}</span>
                    <span className="mcp-server-command">{s.command}</span>
                  </div>
                  <div className="mcp-server-actions">
                    <button
                      className="mcp-btn mcp-btn-restart"
                      onClick={() => handleRestart(s.name)}
                      disabled={mutating.has(s.name)}
                      aria-label={`Restart ${s.name}`}
                    >
                      restart
                    </button>
                    <button
                      className="mcp-btn mcp-btn-remove"
                      onClick={() => handleRemove(s.name)}
                      disabled={mutating.has(s.name)}
                      aria-label={`Remove ${s.name}`}
                    >
                      remove
                    </button>
                  </div>
                </div>
              ))}
            </div>
            {adding ? (
              <div className="mcp-add-form">
                <input
                  className="mcp-input"
                  placeholder="Name"
                  value={newName}
                  onChange={(e) => setNewName(e.target.value)}
                  aria-label="Server name"
                />
                <input
                  className="mcp-input"
                  placeholder="Command (e.g. npx @anthropic/mcp-server)"
                  value={newCommand}
                  onChange={(e) => setNewCommand(e.target.value)}
                  aria-label="Server command"
                />
                <input
                  className="mcp-input"
                  placeholder="Arguments (space-separated, optional)"
                  value={newArgs}
                  onChange={(e) => setNewArgs(e.target.value)}
                  aria-label="Server arguments"
                />
                <div className="mcp-add-actions">
                  <button
                    className="mcp-btn mcp-btn-add-confirm"
                    onClick={handleAdd}
                    disabled={!newName.trim() || !newCommand.trim() || mutating.has("__add__")}
                    aria-label="Add server"
                  >
                    add
                  </button>
                  <button className="mcp-btn mcp-btn-cancel" onClick={() => setAdding(false)} aria-label="Cancel adding server">
                    cancel
                  </button>
                </div>
              </div>
            ) : (
              <button className="mcp-btn mcp-btn-add" onClick={() => setAdding(true)} aria-label="Add MCP server">
                + add server
              </button>
            )}
            </>
            )}
          </section>

          <section className="settings-section">
            <h3>Keyboard Shortcuts</h3>
            <div className="settings-row">
              <span className="settings-label">Toggle sidebar</span>
              <kbd className="settings-kbd">{platformKeys.mod}+B</kbd>
            </div>
            <div className="settings-row">
              <span className="settings-label">Toggle tool output</span>
              <kbd className="settings-kbd">{platformKeys.mod}+/</kbd>
            </div>
            <div className="settings-row">
              <span className="settings-label">Send message</span>
              <kbd className="settings-kbd">Enter</kbd>
            </div>
            <div className="settings-row">
              <span className="settings-label">New line</span>
              <kbd className="settings-kbd">{platformKeys.shift}+Enter</kbd>
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
