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

  // Stable ref-based handler prevents WsClient recreation on callback changes
  const callbacksRef = useRef({ dispatch, onConnected, onError });
  callbacksRef.current = { dispatch, onConnected, onError };

  const handleMessage = useCallback(
    (type: string, payload: unknown) => {
      const { dispatch, onConnected, onError } = callbacksRef.current;
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
          break;
        case "reconnecting":
          break;
        case "connection_lost":
          break;
      }
    },
    []
  );

  useEffect(() => {
    const client = new WsClient(url, handleMessage);
    clientRef.current = client;
    client.connect().catch(() => {
      // connect() rejects on handshake failure; onclose schedules reconnect
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
