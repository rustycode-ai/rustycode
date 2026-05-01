import { useEffect, useRef, useCallback } from "react";
import { WsClient } from "../protocol/ws-client";
import type { FrontendSession, EventPayload } from "../protocol/types";
import type { SessionAction } from "../state/session-store";

interface UseWebSocketOptions {
  url: string;
  dispatch: React.Dispatch<SessionAction>;
  onConnected?: (token: string) => void;
  onError?: (code: string, message: string) => void;
}

export function useWebSocket({ url, dispatch, onConnected, onError }: UseWebSocketOptions) {
  const clientRef = useRef<WsClient | null>(null);

  const handleMessage = useCallback(
    (type: string, payload: unknown) => {
      switch (type) {
        case "session_created":
        case "session_resumed":
          onConnected?.((payload as { session_token: string }).session_token);
          break;
        case "state_snapshot":
          dispatch({ type: "SET_SESSION", session: payload as FrontendSession });
          break;
        case "event":
          dispatch({ type: "APPLY_EVENT", payload: payload as EventPayload });
          break;
        case "error":
          onError?.(
            (payload as { code: string }).code,
            (payload as { message: string }).message
          );
          break;
        case "heartbeat_ack":
          // Measure RTT if needed
          break;
        case "reconnecting":
          console.info("Reconnecting...", payload);
          break;
        case "connection_lost":
          console.error("Connection lost:", payload);
          break;
      }
    },
    [dispatch, onConnected, onError]
  );

  useEffect(() => {
    const client = new WsClient(url, handleMessage);
    clientRef.current = client;
    client.connect().catch((err: unknown) => {
      console.error("Initial connection failed:", err);
    });

    return () => {
      client.disconnect();
      clientRef.current = null;
    };
  }, [url, handleMessage]);

  const sendInput = useCallback((content: string) => {
    clientRef.current?.sendInput(content);
  }, []);

  const sendAbort = useCallback(() => {
    clientRef.current?.sendAbort();
  }, []);

  return { sendInput, sendAbort };
}
