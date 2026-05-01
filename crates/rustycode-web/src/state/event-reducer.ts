// Parts-based event reducer — maps StreamEvent variants to FrontendSession mutations
// using structured MessagePart instead of flat text

import type { FrontendSession, StreamEvent, MessagePart } from "../protocol/types";

export function applyEvent(
  session: FrontendSession,
  event: StreamEvent
): FrontendSession {
  switch (event.type) {
    case "text_delta":
      return appendTextPart(session, event.data.content);

    case "thinking_delta":
      return appendThinkingPart(session, event.data.content);

    case "tool_call_started":
      return startToolCall(session, event.data.id, event.data.name);

    case "tool_input_delta":
      return appendToolInput(session, event.data.id, event.data.chunk);

    case "tool_exec_started":
      return updateToolStatus(session, event.data.id, "running");

    case "tool_exec_completed":
      return completeToolCall(
        session,
        event.data.id,
        event.data.output,
        event.data.is_error
      );

    case "turn_started":
      return session;

    case "token_usage":
      return {
        ...session,
        input_tokens: event.data.input_tokens,
        output_tokens: event.data.output_tokens,
      };

    case "turn_completed":
      return session;

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

/** Append text to the last TextPart in the current assistant message, or create one. */
function appendTextPart(session: FrontendSession, chunk: string): FrontendSession {
  const currentResponse = session.current_response + chunk;
  const messages = updateLastAssistant(session, (msg) => {
    const parts = [...msg.parts];
    const last = parts[parts.length - 1];
    if (last && last.type === "text") {
      parts[parts.length - 1] = { ...last, content: last.content + chunk };
    } else {
      parts.push({ type: "text", content: chunk });
    }
    return { ...msg, content: currentResponse, parts };
  });
  return { ...session, current_response: currentResponse, messages };
}

/** Append thinking content to the last ThinkingPart, or create one. */
function appendThinkingPart(session: FrontendSession, chunk: string): FrontendSession {
  const messages = updateLastAssistant(session, (msg) => {
    const parts = [...msg.parts];
    const last = parts[parts.length - 1];
    if (last && last.type === "thinking") {
      parts[parts.length - 1] = { ...last, content: last.content + chunk };
    } else {
      parts.push({ type: "thinking", content: chunk });
    }
    return { ...msg, parts };
  });
  return { ...session, messages };
}

/** Create a pending ToolCallPart. */
function startToolCall(
  session: FrontendSession,
  toolId: string,
  toolName: string
): FrontendSession {
  const messages = updateLastAssistant(session, (msg) => {
    const part: MessagePart = {
      type: "tool_call",
      id: toolId,
      name: toolName,
      status: "pending",
      startedAt: Date.now(),
    };
    return { ...msg, parts: [...msg.parts, part] };
  });
  return {
    ...session,
    messages,
    tool_iteration_count: session.tool_iteration_count + 1,
  };
}

/** Append input chunks to a tool call. */
function appendToolInput(
  session: FrontendSession,
  toolId: string,
  chunk: string
): FrontendSession {
  const messages = updateLastAssistant(session, (msg) => {
    const parts = msg.parts.map((p) =>
      p.type === "tool_call" && p.id === toolId
        ? { ...p, input: (p.input ?? "") + chunk }
        : p
    );
    return { ...msg, parts };
  });
  return { ...session, messages };
}

/** Update a tool call's status. */
function updateToolStatus(
  session: FrontendSession,
  toolId: string,
  status: "running" | "pending"
): FrontendSession {
  const messages = updateLastAssistant(session, (msg) => {
    const parts = msg.parts.map((p) =>
      p.type === "tool_call" && p.id === toolId ? { ...p, status } : p
    );
    return { ...msg, parts };
  });
  return { ...session, messages };
}

/** Complete a tool call with output. */
function completeToolCall(
  session: FrontendSession,
  toolId: string,
  output: string,
  isError: boolean
): FrontendSession {
  const messages = updateLastAssistant(session, (msg) => {
    const parts = msg.parts.map((p) =>
      p.type === "tool_call" && p.id === toolId
        ? {
            ...p,
            status: isError ? "error" as const : "completed" as const,
            output,
            completedAt: Date.now(),
          }
        : p
    );
    return { ...msg, parts };
  });
  return { ...session, messages };
}

/** Helper: update the last assistant message immutably. */
function updateLastAssistant(
  session: FrontendSession,
  updater: (msg: FrontendSession["messages"][number]) => FrontendSession["messages"][number]
): FrontendSession["messages"] {
  const idx = session.messages.length - 1;
  const last = session.messages[idx];
  if (!last || last.kind !== "Assistant") return session.messages;
  return session.messages.map((m, i) => (i === idx ? updater(m) : m));
}

/** Finalize the assistant message content on done/end_turn. */
function finalizeResponse(
  session: FrontendSession,
  content: string
): FrontendSession {
  const messages = session.messages.map((m, i) => {
    if (i !== session.messages.length - 1 || m.kind !== "Assistant") return m;
    // If no parts were created (flat text fallback), add a text part
    const parts = m.parts.length > 0 ? m.parts : [{ type: "text" as const, content }];
    return { ...m, content, parts };
  });
  return { ...session, messages, current_response: content };
}
