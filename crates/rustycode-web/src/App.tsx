import { useState, useEffect, useCallback, useRef } from "react";
import { SessionContext, useSession } from "./state/session-store";
import { useSessionProvider } from "./hooks/useSession";
import { useToast } from "./hooks/useToast";
import { StatusBar } from "./components/StatusBar";
import { MessageList } from "./components/MessageList";
import { InputBar } from "./components/InputBar";
import { SessionSidebar } from "./components/SessionSidebar";
import { SettingsPanel } from "./components/SettingsPanel";
import { CommandPalette } from "./components/CommandPalette";
import { PlanBanner } from "./components/PlanBanner";
import { ToolApprovalModal } from "./components/ToolApprovalModal";
import { ToastContainer } from "./components/ToastContainer";

interface AppInnerProps {
  pendingApproval: import("./protocol/types").ToolApprovalRequestPayload | null;
  handleToolApprovalResponse: (requestId: string, approved: boolean) => void;
  sendPlanApproval: (planId: string, approved: boolean) => void;
  connectionStatus: import("./components/StatusBar").ConnectionStatus;
}

function AppInner({ pendingApproval, handleToolApprovalResponse, sendPlanApproval, connectionStatus }: AppInnerProps) {
  const { state, sendInput, sendAbort, getSessionToken } = useSession();
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const [providerInfo, setProviderInfo] = useState({ provider: "", model: "" });
  const [toolOutputsVisible, setToolOutputsVisible] = useState(true);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const { toasts, addToast, dismissToast } = useToast();
  const prevStatusRef = useRef(connectionStatus);
  const mainRef = useRef<HTMLDivElement>(null);

  const toggleSidebar = useCallback(() => {
    setSidebarOpen((prev) => !prev);
  }, []);

  useEffect(() => {
    const prev = prevStatusRef.current;
    prevStatusRef.current = connectionStatus;
    if (connectionStatus === "disconnected" && prev !== "disconnected") {
      addToast("Connection lost — reconnecting…", "error");
    } else if (connectionStatus === "connected" && prev === "disconnected") {
      addToast("Reconnected", "success");
    }
  }, [connectionStatus, addToast]);

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
      if ((e.metaKey || e.ctrlKey) && e.key === "k") {
        e.preventDefault();
        setPaletteOpen((prev) => !prev);
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [toggleSidebar]);

  const handleModelSwitch = useCallback((provider: string, model: string) => {
    setProviderInfo({ provider, model });
  }, []);

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
        onModelSwitch={handleModelSwitch}
        provider={providerInfo.provider}
        model={providerInfo.model}
        connectionStatus={connectionStatus}
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
        <main className="main" id="main-content" ref={mainRef}>
          <MessageList messages={state.messages} toolOutputsVisible={toolOutputsVisible} pending={state.pending_request} scrollContainerRef={mainRef} />
        </main>
      </div>
      {state.plan && (
        <PlanBanner
          plan={state.plan}
          onApprove={(planId) => sendPlanApproval(planId, true)}
          onReject={(planId) => sendPlanApproval(planId, false)}
        />
      )}
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
      <CommandPalette
        open={paletteOpen}
        onClose={() => setPaletteOpen(false)}
        sessionToken={getSessionToken() ?? ""}
        onToggleSidebar={toggleSidebar}
        onToggleToolOutputs={() => setToolOutputsVisible((prev) => !prev)}
        onOpenModelSelector={() => {
          // Trigger model selector by clicking it programmatically
          document.querySelector<HTMLElement>(".model-selector-btn")?.click();
        }}
      />
      <ToolApprovalModal
        request={pendingApproval}
        onRespond={handleToolApprovalResponse}
      />
      <ToastContainer toasts={toasts} onDismiss={dismissToast} />
    </div>
  );
}

export default function App() {
  const { contextValue, pendingApproval, handleToolApprovalResponse, sendPlanApproval, connectionStatus } = useSessionProvider();

  return (
    <SessionContext.Provider value={contextValue}>
      <AppInner pendingApproval={pendingApproval} handleToolApprovalResponse={handleToolApprovalResponse} sendPlanApproval={sendPlanApproval} connectionStatus={connectionStatus} />
    </SessionContext.Provider>
  );
}
