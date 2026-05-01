import { useState, useEffect, useCallback } from "react";
import { SessionContext, useSession } from "./state/session-store";
import { useSessionProvider } from "./hooks/useSession";
import { StatusBar } from "./components/StatusBar";
import { MessageList } from "./components/MessageList";
import { InputBar } from "./components/InputBar";
import { SessionSidebar } from "./components/SessionSidebar";
import { SettingsPanel } from "./components/SettingsPanel";

function AppInner() {
  const { state, sendInput, sendAbort } = useSession();
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const [providerInfo, setProviderInfo] = useState({ provider: "", model: "" });
  const [toolOutputsVisible, setToolOutputsVisible] = useState(true);
  const [settingsOpen, setSettingsOpen] = useState(false);

  const toggleSidebar = useCallback(() => {
    setSidebarOpen((prev) => !prev);
  }, []);

  useEffect(() => {
    fetch("/api/providers")
      .then((r) => r.json())
      .then((d) => setProviderInfo({ provider: d.provider || "", model: d.model || "" }))
      .catch(() => {});
  }, []);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "b") {
        e.preventDefault();
        toggleSidebar();
      }
      if ((e.metaKey || e.ctrlKey) && e.key === "/") {
        e.preventDefault();
        setToolOutputsVisible((prev) => !prev);
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [toggleSidebar]);

  const handleNewSession = () => {
    window.location.reload();
  };

  const handleSelectSession = (id: string) => {
    const url = new URL(window.location.href);
    url.searchParams.set("session", id);
    window.location.href = url.toString();
  };

  return (
    <div className="app">
      <a className="skip-link" href="#main-content">Skip to content</a>
      <StatusBar
        toolIterationCount={state.tool_iteration_count}
        pending={state.pending_request}
        inputTokens={state.input_tokens}
        outputTokens={state.output_tokens}
        onToggleSidebar={toggleSidebar}
        onOpenSettings={() => setSettingsOpen(true)}
        provider={providerInfo.provider}
        model={providerInfo.model}
      />
      <div className="app-body">
        {sidebarOpen && (
          <SessionSidebar
            currentSessionId={null}
            onSelectSession={handleSelectSession}
            onNewSession={handleNewSession}
            open={sidebarOpen}
            onClose={() => setSidebarOpen(false)}
          />
        )}
        <main className="main" id="main-content">
          <MessageList messages={state.messages} toolOutputsVisible={toolOutputsVisible} />
        </main>
      </div>
      <footer className="footer">
        <InputBar
          onSend={sendInput}
          onAbort={sendAbort}
          pending={state.pending_request}
        />
      </footer>
      <SettingsPanel
        provider={providerInfo.provider}
        model={providerInfo.model}
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
      />
    </div>
  );
}

export default function App() {
  const { contextValue } = useSessionProvider();

  return (
    <SessionContext.Provider value={contextValue}>
      <AppInner />
    </SessionContext.Provider>
  );
}
