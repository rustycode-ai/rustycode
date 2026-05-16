import { describe, it, expect, vi, beforeEach } from "vitest";
import { sessionReducer, initialSession } from "../session-store";
import type { FrontendSession } from "../../protocol/types";

beforeEach(() => {
  let id = 0;
  vi.stubGlobal("crypto", {
    randomUUID: () => `test-uuid-${++id}`,
  });
});

function session(overrides?: Partial<FrontendSession>): FrontendSession {
  return { ...initialSession, ...overrides };
}

describe("sessionReducer", () => {
  it("returns initial session for unknown action", () => {
    const result = sessionReducer(initialSession, { type: "UNKNOWN" } as never);
    expect(result).toEqual(initialSession);
  });

  it("SET_SESSION normalizes messages with id and parts", () => {
    const result = sessionReducer(initialSession, {
      type: "SET_SESSION",
      session: {
        ...initialSession,
        messages: [
          { content: "hi", kind: "User" },
          { content: "hello", kind: "Assistant" },
        ],
      } as FrontendSession,
    });
    expect(result.messages).toHaveLength(2);
    expect(result.messages[0]?.id).toBe("snapshot-0");
    expect(result.messages[0]?.parts).toEqual([]);
    expect(result.messages[1]?.id).toBe("snapshot-1");
  });

  it("SET_SESSION preserves existing ids and parts", () => {
    const result = sessionReducer(initialSession, {
      type: "SET_SESSION",
      session: {
        ...initialSession,
        messages: [
          { id: "m1", content: "hi", kind: "User", parts: [{ type: "text", content: "hi" }] },
        ],
      } as FrontendSession,
    });
    expect(result.messages[0]?.id).toBe("m1");
    expect(result.messages[0]?.parts).toEqual([{ type: "text", content: "hi" }]);
  });

  it("SET_INPUT updates input field", () => {
    const result = sessionReducer(initialSession, { type: "SET_INPUT", input: "hello" });
    expect(result.input).toBe("hello");
  });

  it("CLEAR_INPUT resets input", () => {
    const s = session({ input: "typing..." });
    const result = sessionReducer(s, { type: "CLEAR_INPUT" });
    expect(result.input).toBe("");
  });

  it("SET_PENDING toggles pending state", () => {
    const result = sessionReducer(initialSession, { type: "SET_PENDING", pending: true });
    expect(result.pending_request).toBe(true);
  });

  it("ADD_USER_MESSAGE appends user and assistant messages", () => {
    const result = sessionReducer(initialSession, {
      type: "ADD_USER_MESSAGE",
      content: "hello world",
    });
    expect(result.messages).toHaveLength(2);
    expect(result.messages[0]?.kind).toBe("User");
    expect(result.messages[0]?.content).toBe("hello world");
    expect(result.messages[1]?.kind).toBe("Assistant");
    expect(result.messages[1]?.content).toBe("");
    expect(result.messages[1]?.parts).toEqual([]);
  });

  it("ADD_USER_MESSAGE sets last_user_prompt", () => {
    const result = sessionReducer(initialSession, {
      type: "ADD_USER_MESSAGE",
      content: "test prompt",
    });
    expect(result.last_user_prompt).toBe("test prompt");
  });

  it("ADD_USER_MESSAGE preserves existing messages", () => {
    const existing = session({
      messages: [{ id: "a1", content: "old", kind: "Assistant", parts: [] }],
    });
    const result = sessionReducer(existing, {
      type: "ADD_USER_MESSAGE",
      content: "new",
    });
    expect(result.messages).toHaveLength(3);
    expect(result.messages[0]?.id).toBe("a1");
  });

  it("ADD_USER_MESSAGE creates unique ids for user and assistant", () => {
    const result = sessionReducer(initialSession, {
      type: "ADD_USER_MESSAGE",
      content: "test",
    });
    expect(result.messages[0]?.id).not.toBe(result.messages[1]?.id);
  });

  it("APPLY_EVENT delegates to applyEvent", () => {
    const s = session({
      messages: [{ id: "a1", content: "", kind: "Assistant", parts: [] }],
    });
    const result = sessionReducer(s, {
      type: "APPLY_EVENT",
      payload: {
        seq: 1,
        event: { type: "text_delta", data: { content: "hi" } },
      },
    });
    expect(result.current_response).toBe("hi");
    expect(result.messages[0]?.parts).toEqual([{ type: "text", content: "hi" }]);
  });

  it("SET_SESSION with empty messages array", () => {
    const result = sessionReducer(session({ messages: [{ id: "x", content: "old", kind: "User", parts: [] }] }), {
      type: "SET_SESSION",
      session: { ...initialSession, messages: [] },
    });
    expect(result.messages).toEqual([]);
  });

  it("ADD_USER_MESSAGE creates text part in user message", () => {
    const result = sessionReducer(initialSession, {
      type: "ADD_USER_MESSAGE",
      content: "hello",
    });
    expect(result.messages[0]?.parts).toEqual([{ type: "text", content: "hello" }]);
  });

  it("consecutive ADD_USER_MESSAGE creates alternating pairs", () => {
    let s = sessionReducer(initialSession, { type: "ADD_USER_MESSAGE", content: "first" });
    s = sessionReducer(s, { type: "ADD_USER_MESSAGE", content: "second" });
    expect(s.messages).toHaveLength(4);
    expect(s.messages[0]?.kind).toBe("User");
    expect(s.messages[1]?.kind).toBe("Assistant");
    expect(s.messages[2]?.kind).toBe("User");
    expect(s.messages[3]?.kind).toBe("Assistant");
    expect(s.last_user_prompt).toBe("second");
  });

  it("SET_SESSION merges with initialSession defaults", () => {
    const partial = { messages: [], input: "test" } as FrontendSession;
    const result = sessionReducer(initialSession, { type: "SET_SESSION", session: partial });
    expect(result.input).toBe("test");
    expect(result.pending_request).toBe(false);
    expect(result.tool_iteration_count).toBe(0);
  });
});
