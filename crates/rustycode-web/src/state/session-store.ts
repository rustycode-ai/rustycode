import { createContext, useContext } from "react";
import type { FrontendSession, EventPayload } from "../protocol/types";
import { applyEvent } from "./event-reducer";

function randomUUID(): string {
  if (typeof crypto !== "undefined" && crypto.randomUUID) {
    return crypto.randomUUID();
  }
  return `${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

export type SessionAction =
  | { type: "SET_SESSION"; session: FrontendSession }
  | { type: "APPLY_EVENT"; payload: EventPayload }
  | { type: "SET_INPUT"; input: string }
  | { type: "SET_PENDING"; pending: boolean }
  | { type: "CLEAR_INPUT" }
  | { type: "ADD_USER_MESSAGE"; content: string };

export const initialSession: FrontendSession = {
  input: "",
  messages: [],
  last_user_prompt: null,
  pending_request: false,
  tool_iteration_count: 0,
  current_response: "",
  input_tokens: 0,
  output_tokens: 0,
  cache_read_tokens: 0,
  cache_creation_tokens: 0,
  plan: null,
};

export function sessionReducer(
  state: FrontendSession,
  action: SessionAction
): FrontendSession {
  switch (action.type) {
    case "SET_SESSION": {
      const session = action.session;
      const messages = session.messages.map((m, i) => ({
        ...m,
        id: m.id || `snapshot-${i}`,
        parts: m.parts || [],
      }));
      return { ...initialSession, ...session, messages };
    }
    case "APPLY_EVENT":
      return applyEvent(state, action.payload.event);
    case "SET_INPUT":
      return { ...state, input: action.input };
    case "SET_PENDING":
      return { ...state, pending_request: action.pending };
    case "CLEAR_INPUT":
      return { ...state, input: "" };
    case "ADD_USER_MESSAGE":
      return {
        ...state,
        last_user_prompt: action.content,
        messages: [
          ...state.messages,
          {       id: randomUUID(), content: action.content, kind: "User" as const, parts: [{ type: "text", content: action.content }], created_at: Date.now() },
          { id: randomUUID(), content: "", kind: "Assistant" as const, parts: [], created_at: Date.now() },
        ],
      };
    default:
      return state;
  }
}

// Context for dependency injection
interface SessionContextValue {
  state: FrontendSession;
  dispatch: React.Dispatch<SessionAction>;
  sendInput: (content: string) => void;
  sendAbort: () => void;
  getSessionToken: () => string | null;
}

export const SessionContext = createContext<SessionContextValue | null>(null);

export function useSession(): SessionContextValue {
  const ctx = useContext(SessionContext);
  if (!ctx) throw new Error("useSession must be used within SessionProvider");
  return ctx;
}
