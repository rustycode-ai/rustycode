import { SessionContext, useSession } from "./state/session-store";
import { useSessionProvider } from "./hooks/useSession";
import { StatusBar } from "./components/StatusBar";
import { MessageList } from "./components/MessageList";
import { InputBar } from "./components/InputBar";

function AppInner() {
  const { state, sendInput, sendAbort } = useSession();

  return (
    <div className="app">
      <StatusBar
        toolIterationCount={state.tool_iteration_count}
        pending={state.pending_request}
      />
      <main className="main">
        <MessageList messages={state.messages} />
      </main>
      <footer className="footer">
        <InputBar
          onSend={sendInput}
          onAbort={sendAbort}
          pending={state.pending_request}
        />
      </footer>
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
