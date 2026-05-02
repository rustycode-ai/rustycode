import { useEffect, useRef, useCallback } from "react";
import { WsClient, classifyError, type ClassifiedError } from "../protocol/ws-client";
import type { FrontendSession, EventPayload, ToolApprovalRequestPayload } from "../protocol/types";
import type { SessionAction } from "../state/session-store";

export type ConnectionStatus = "connected" | "connecting" | "disconnected";

interface UseWebSocketOptions {
  url: string;
  dispatch: React.Dispatch<SessionAction>;
  onConnected?: (token: string) => void;
  onError?: (error: ClassifiedError) => void;
  onToolApprovalRequest?: (request: ToolApprovalRequestPayload) => void;
  onConnectionChange?: (status: "connected" | "connecting" | "disconnected") => void;
}

const SESSION_KEY = "rustycode-session-token";

export function clearSessionToken() {
  try { localStorage.removeItem(SESSION_KEY); } catch {}
}

export function getSavedSessionToken(): string | null {
  try { return localStorage.getItem(SESSION_KEY); } catch { return null; }
}

function saveSessionToken(token: string) {
  try { localStorage.setItem(SESSION_KEY, token); } catch {}
}

export function useWebSocket({ url, dispatch, onConnected, onError, onToolApprovalRequest, onConnectionChange }: UseWebSocketOptions) {
  const clientRef = useRef<WsClient | null>(null);

  const callbacksRef = useRef({ dispatch, onConnected, onError, onToolApprovalRequest, onConnectionChange });
  callbacksRef.current = { dispatch, onConnected, onError, onToolApprovalRequest, onConnectionChange };

  const handleMessage = useCallback(
    (type: string, payload: unknown) => {
      const { dispatch, onConnected, onError, onToolApprovalRequest } = callbacksRef.current;
      switch (type) {
        case "session_created":
        case "session_resumed": {
          const token = (payload as { session_token: string }).session_token;
          saveSessionToken(token);
          onConnected?.(token);
          break;
        }
        case "state_snapshot":
          dispatch({ type: "SET_SESSION", session: payload as FrontendSession });
          break;
        case "event":
          dispatch({ type: "APPLY_EVENT", payload: payload as EventPayload });
          break;
        case "error":
          onError?.(
            classifyError(
              (payload as { code: string }).code,
              (payload as { message: string }).message
            )
          );
          break;
        case "heartbeat_ack":
          break;
        case "tool_approval_requested":
          onToolApprovalRequest?.(payload as ToolApprovalRequestPayload);
          break;
        case "reconnecting":
          callbacksRef.current.onConnectionChange?.("connecting");
          break;
        case "connection_lost":
          callbacksRef.current.onConnectionChange?.("disconnected");
          break;
      }
    },
    []
  );

  useEffect(() => {
    callbacksRef.current.onConnectionChange?.("connecting");
    const client = new WsClient(url, handleMessage);
    clientRef.current = client;
    client.connect().then(
      () => { callbacksRef.current.onConnectionChange?.("connected"); },
      () => { callbacksRef.current.onConnectionChange?.("disconnected"); },
    );

    return () => {
      client.disconnect();
      clientRef.current = null;
      callbacksRef.current.onConnectionChange?.("disconnected");
    };
  }, [url, handleMessage]);

  const sendInput = useCallback((content: string) => {
    clientRef.current?.sendInput(content);
  }, []);

  const sendAbort = useCallback(() => {
    clientRef.current?.sendAbort();
  }, []);

  const sendToolApproval = useCallback((requestId: string, approved: boolean) => {
    clientRef.current?.sendToolApproval(requestId, approved);
  }, []);

  const sendPlanApproval = useCallback((planId: string, approved: boolean) => {
    clientRef.current?.sendPlanApproval(planId, approved);
  }, []);

  return { sendInput, sendAbort, sendToolApproval, sendPlanApproval, getSessionToken: () => clientRef.current?.token ?? null };
}
