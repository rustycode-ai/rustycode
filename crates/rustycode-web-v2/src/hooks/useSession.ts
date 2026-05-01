import { useReducer, useMemo } from "react";
import {
  sessionReducer,
  initialSession,
} from "../state/session-store";
import { useWebSocket } from "./useWebSocket";

const WS_URL =
  typeof window !== "undefined"
    ? `${window.location.protocol === "https:" ? "wss:" : "ws:"}//${window.location.host}/ws`
    : "ws://127.0.0.1:8080/ws";

export function useSessionProvider() {
  const [state, dispatch] = useReducer(sessionReducer, initialSession);
  const { sendInput, sendAbort } = useWebSocket({
    url: WS_URL,
    dispatch,
  });

  const handleSendInput = (content: string) => {
    dispatch({ type: "CLEAR_INPUT" });
    dispatch({ type: "SET_PENDING", pending: true });
    sendInput(content);
  };

  const contextValue = useMemo(
    () => ({ state, dispatch, sendInput: handleSendInput, sendAbort }),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [state, sendAbort]
  );

  return { state, contextValue, sendAbort };
}
