import { useState, useEffect, useCallback } from "react";

interface SessionInfo {
  id: string;
  created_at: string;
  last_active_at: string;
  message_count: number;
  client_count: number;
}

interface SessionSidebarProps {
  currentSessionId: string | null;
  onSelectSession: (id: string) => void;
  onNewSession: () => void;
  open: boolean;
  onClose: () => void;
}

function formatTimeAgo(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime();
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return "just now";
  if (mins < 60) return `${mins}m ago`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs}h ago`;
  return `${Math.floor(hrs / 24)}d ago`;
}

export function SessionSidebar({
  currentSessionId,
  onSelectSession,
  onNewSession,
  open,
  onClose,
}: SessionSidebarProps) {
  const [sessions, setSessions] = useState<SessionInfo[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(false);

  const fetchSessions = useCallback(async () => {
    setLoading(true);
    setError(false);
    try {
      const res = await fetch("/api/sessions");
      if (res.ok) {
        const data = await res.json();
        setSessions(data);
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
    if (open) fetchSessions();
  }, [open, fetchSessions]);

  const handleDelete = async (id: string, e: React.MouseEvent) => {
    e.stopPropagation();
    try {
      const res = await fetch(`/api/sessions/${id}`, { method: "DELETE" });
      if (res.ok) {
        setSessions((prev) => prev.filter((s) => s.id !== id));
      }
    } catch {
      // ignore
    }
  };

  if (!open) return null;

  return (
    <aside className="session-sidebar" role="complementary" aria-label="Sessions">
      <div className="sidebar-header">
        <h2>Sessions</h2>
        <div className="sidebar-actions">
          <button
            className="btn-icon"
            onClick={onNewSession}
            aria-label="New session"
            title="New session"
          >
            +
          </button>
          <button
            className="btn-icon"
            onClick={onClose}
            aria-label="Close sidebar"
            title="Close sidebar"
          >
            x
          </button>
        </div>
      </div>
      <ul className="sidebar-list" role="listbox" aria-label="Session list">
        {loading && sessions.length === 0 ? (
          <li className="sidebar-loading">
            <span className="sidebar-shimmer" />
            Loading sessions…
          </li>
        ) : error && sessions.length === 0 ? (
          <li className="sidebar-error">
            Failed to load sessions
            <button className="sidebar-retry" onClick={fetchSessions} type="button">Retry</button>
          </li>
        ) : sessions.length === 0 ? (
          <li className="sidebar-empty">No sessions yet</li>
        ) : (
          sessions.map((s) => (
            <li
              key={s.id}
              className={`sidebar-item ${s.id === currentSessionId ? "sidebar-active" : ""}`}
              role="option"
              aria-selected={s.id === currentSessionId}
              tabIndex={0}
              onClick={() => {
                onSelectSession(s.id);
                onClose();
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  onSelectSession(s.id);
                  onClose();
                }
              }}
            >
              <div className="sidebar-item-title">
                {s.message_count > 0
                  ? `${s.message_count} message${s.message_count !== 1 ? "s" : ""}`
                  : "New session"}
              </div>
              <div className="sidebar-item-meta">
                <span className="sidebar-item-id">{s.id.slice(0, 8)}</span>
                <span>{formatTimeAgo(s.last_active_at)}</span>
                {s.client_count > 0 && (
                  <span className="sidebar-connected">● live</span>
                )}
              </div>
              <button
                className="sidebar-delete"
                onClick={(e) => handleDelete(s.id, e)}
                aria-label={`Delete session ${s.id.slice(0, 8)}`}
                title="Delete session"
              >
                ×
              </button>
            </li>
          ))
        )}
      </ul>
    </aside>
  );
}
