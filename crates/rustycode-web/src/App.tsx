import { useState, useEffect, useCallback, useRef, lazy, Suspense } from "react";
import { SessionContext, useSession } from "./state/session-store";
import { useSessionProvider } from "./hooks/useSession";
import { useToast } from "./hooks/useToast";
import { StatusBar } from "./components/StatusBar";
import { MessageList } from "./components/MessageList";
import { InputBar } from "./components/InputBar";
import { SessionSidebar } from "./components/SessionSidebar";
import { PlanBanner } from "./components/PlanBanner";
import { ToastContainer } from "./components/ToastContainer";
import { SectionErrorBoundary } from "./components/SectionErrorBoundary";

const SettingsPanel = lazy(() => import("./components/SettingsPanel").then(m => ({ default: m.SettingsPanel })));
const CommandPalette = lazy(() => import("./components/CommandPalette").then(m => ({ default: m.CommandPalette })));
const ToolApprovalModal = lazy(() => import("./components/ToolApprovalModal").then(m => ({ default: m.ToolApprovalModal })));
const ShortcutsOverlay = lazy(() => import("./components/ShortcutsOverlay").then(m => ({ default: m.ShortcutsOverlay })));
const SearchOverlay = lazy(() => import("./components/SearchOverlay").then(m => ({ default: m.SearchOverlay })));

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
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
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
      .then((d) => setProviderInfo({
        provider: d.current?.provider || "",
        model: d.current?.model || "",
      }))
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
        setShortcutsOpen((prev) => !prev);
      }
      if ((e.metaKey || e.ctrlKey) && e.key === "k") {
        e.preventDefault();
        setPaletteOpen((prev) => !prev);
      }
      if ((e.metaKey || e.ctrlKey) && e.key === "f") {
        e.preventDefault();
        setSearchOpen(true);
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [toggleSidebar]);

  const handleModelSwitch = useCallback((provider: string, model: string) => {
    setProviderInfo({ provider, model });
  }, []);

  const handleRegenerate = useCallback(() => {
    if (state.pending_request) return;
    const lastUserMsg = [...state.messages].reverse().find((m) => m.kind === "User");
    if (!lastUserMsg?.content) return;
    sendAbort();
    setTimeout(() => sendInput(lastUserMsg.content), 100);
  }, [state.pending_request, state.messages, sendAbort, sendInput]);

  const handleNewSession = () => {
    window.location.reload();
  };

  const handleSelectSession = (id: string) => {
    const url = new URL(window.location.href);
    url.searchParams.set("session", id);
    window.location.href = url.toString();
  };

  const handleSearchNavigate = useCallback((messageId: string) => {
    const el = document.querySelector(`[data-message-id="${messageId}"]`);
    if (el) {
      el.scrollIntoView({ behavior: "smooth", block: "center" });
      el.classList.add("message-highlight-flash");
      setTimeout(() => el.classList.remove("message-highlight-flash"), 2000);
    }
  }, []);

  return (
    <div className="app">
      <a className="skip-link" href="#main-content">Skip to content</a>
      <StatusBar
        toolIterationCount={state.tool_iteration_count}
        pending={state.pending_request}
        inputTokens={state.input_tokens}
        outputTokens={state.output_tokens}
        cacheReadTokens={state.cache_read_tokens}
        cacheCreationTokens={state.cache_creation_tokens}
        onToggleSidebar={toggleSidebar}
        onOpenSettings={() => setSettingsOpen(true)}
        onModelSwitch={handleModelSwitch}
        provider={providerInfo.provider}
        model={providerInfo.model}
        connectionStatus={connectionStatus}
      />
      <div className="app-body">
        {sidebarOpen && (
          <>
            <div className="sidebar-backdrop" onClick={() => setSidebarOpen(false)} />
            <SectionErrorBoundary name="Sidebar">
              <SessionSidebar
                currentSessionId={null}
                onSelectSession={handleSelectSession}
                onNewSession={handleNewSession}
                open={sidebarOpen}
                onClose={() => setSidebarOpen(false)}
              />
            </SectionErrorBoundary>
          </>
        )}
        <main className="main" id="main-content" ref={mainRef}>
          <SectionErrorBoundary name="Messages">
            <MessageList messages={state.messages} toolOutputsVisible={toolOutputsVisible} pending={state.pending_request} scrollContainerRef={mainRef} />
          </SectionErrorBoundary>
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
          onRegenerate={handleRegenerate}
        />
      </footer>
      {settingsOpen && (
        <Suspense fallback={null}>
          <SettingsPanel
            provider={providerInfo.provider}
            model={providerInfo.model}
            open={settingsOpen}
            onClose={() => setSettingsOpen(false)}
          />
        </Suspense>
      )}
      {paletteOpen && (
        <Suspense fallback={null}>
          <CommandPalette
            open={paletteOpen}
            onClose={() => setPaletteOpen(false)}
            sessionToken={getSessionToken() ?? ""}
            messages={state.messages}
            onToggleSidebar={toggleSidebar}
            onToggleToolOutputs={() => setToolOutputsVisible((prev) => !prev)}
            onOpenModelSelector={() => {
              document.querySelector<HTMLElement>(".model-selector-btn")?.click();
            }}
          />
        </Suspense>
      )}
      {pendingApproval && (
        <Suspense fallback={null}>
          <ToolApprovalModal
            request={pendingApproval}
            onRespond={handleToolApprovalResponse}
          />
        </Suspense>
      )}
      <ToastContainer toasts={toasts} onDismiss={dismissToast} />
      {shortcutsOpen && (
        <Suspense fallback={null}>
          <ShortcutsOverlay onClose={() => setShortcutsOpen(false)} />
        </Suspense>
      )}
      {searchOpen && (
        <Suspense fallback={null}>
          <SearchOverlay
            messages={state.messages}
            onClose={() => setSearchOpen(false)}
            onNavigate={handleSearchNavigate}
          />
        </Suspense>
      )}
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
