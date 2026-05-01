import { useReducer, useMemo, useState, useCallback } from "react";
import {
  sessionReducer,
  initialSession,
} from "../state/session-store";
import { useWebSocket } from "./useWebSocket";
import type { ConnectionStatus } from "./useWebSocket";
import type { ToolApprovalRequestPayload } from "../protocol/types";

const WS_URL =
  typeof window !== "undefined"
    ? `${window.location.protocol === "https:" ? "wss:" : "ws:"}//${window.location.host}/ws`
    : "ws://127.0.0.1:8080/ws";

export function useSessionProvider() {
  const [state, dispatch] = useReducer(sessionReducer, initialSession);
  const [pendingApproval, setPendingApproval] = useState<ToolApprovalRequestPayload | null>(null);
  const [connectionStatus, setConnectionStatus] = useState<ConnectionStatus>("connecting");

  const handleToolApprovalRequest = useCallback((request: ToolApprovalRequestPayload) => {
    setPendingApproval(request);
  }, []);

  const handleConnectionChange = useCallback((status: ConnectionStatus) => {
    setConnectionStatus(status);
  }, []);

  const { sendInput, sendAbort, sendToolApproval, sendPlanApproval, getSessionToken } = useWebSocket({
    url: WS_URL,
    dispatch,
    onToolApprovalRequest: handleToolApprovalRequest,
    onConnectionChange: handleConnectionChange,
  });

  const handleSendInput = useCallback((content: string) => {
    dispatch({ type: "ADD_USER_MESSAGE", content });
    dispatch({ type: "CLEAR_INPUT" });
    dispatch({ type: "SET_PENDING", pending: true });
    sendInput(content);
  }, [sendInput]);

  const handleToolApprovalResponse = useCallback((requestId: string, approved: boolean) => {
    sendToolApproval(requestId, approved);
    setPendingApproval(null);
  }, [sendToolApproval]);

  const contextValue = useMemo(
    () => ({
      state,
      dispatch,
      sendInput: handleSendInput,
      sendAbort,
      getSessionToken,
    }),
    [state, sendAbort, handleSendInput]
  );

  return { state, contextValue, sendAbort, sendPlanApproval, getSessionToken, pendingApproval, handleToolApprovalResponse, connectionStatus };
}
