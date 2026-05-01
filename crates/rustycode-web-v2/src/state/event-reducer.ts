// TypeScript re-implementation of the Rust accumulator from
// crates/rustycode-ui-model/src/accumulator.rs
// Maps StreamEvent variants to FrontendSession mutations

import type { FrontendSession, StreamEvent } from "../protocol/types";

export function applyEvent(
  session: FrontendSession,
  event: StreamEvent
): FrontendSession {
  switch (event.type) {
    case "text_delta":
      return appendChunk(session, event.data.content);

    case "thinking_delta":
      return appendChunk(session, event.data.content);

    case "tool_call_started": {
      let s = {
        ...session,
        tool_iteration_count: session.tool_iteration_count + 1,
      };
      s = appendChunk(s, `\n[tool: ${event.data.name}]\n`);
      return s;
    }

    case "tool_input_delta":
      return session;

    case "tool_exec_started":
      return session;

    case "tool_exec_completed": {
      const { name, output, is_error } = event.data;
      const kind = is_error ? "Error" as const : "Tool" as const;
      const content = is_error
        ? `[tool error: ${name}] ${output}`
        : `[tool: ${name}] ${output}`;
      return {
        ...session,
        messages: [...session.messages, { content, kind }],
      };
    }

    case "turn_started":
      return session;

    case "token_usage":
      return session;

    case "turn_completed": {
      if (event.data.stop_reason === "end_turn") {
        const content = session.current_response;
        if (content) {
          return finalizeResponse(session, content);
        }
      }
      return session;
    }

    case "cache_usage":
      return session;

    case "done": {
      let s = session;
      if (s.current_response) {
        s = finalizeResponse(s, s.current_response);
      }
      return { ...s, pending_request: false };
    }

    default:
      return session;
  }
}

function appendChunk(session: FrontendSession, chunk: string): FrontendSession {
  const currentResponse = session.current_response + chunk;
  const messages = session.messages.map((m, i) =>
    i === session.messages.length - 1 && m.kind === "Assistant"
      ? { ...m, content: currentResponse }
      : m
  );
  return { ...session, current_response: currentResponse, messages };
}

function finalizeResponse(
  session: FrontendSession,
  content: string
): FrontendSession {
  const messages = session.messages.map((m, i) =>
    i === session.messages.length - 1 && m.kind === "Assistant"
      ? { ...m, content }
      : m
  );
  return {
    ...session,
    messages,
    current_response: content,
    pending_request: false,
  };
}
