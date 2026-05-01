import { createContext, useContext } from "react";
import type { FrontendSession, EventPayload } from "../protocol/types";
import { applyEvent } from "./event-reducer";

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
};

export function sessionReducer(
  state: FrontendSession,
  action: SessionAction
): FrontendSession {
  switch (action.type) {
    case "SET_SESSION":
      return action.session;
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
          { content: action.content, kind: "User" as const },
          { content: "", kind: "Assistant" as const },
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
}

export const SessionContext = createContext<SessionContextValue | null>(null);

export function useSession(): SessionContextValue {
  const ctx = useContext(SessionContext);
  if (!ctx) throw new Error("useSession must be used within SessionProvider");
  return ctx;
}
