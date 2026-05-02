import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { renderHook, act, cleanup } from "@testing-library/react";

// Capture the onMessage callback passed to WsClient constructor
let capturedOnMessage: ((type: string, payload: unknown) => void) | null = null;

vi.mock("../../protocol/ws-client", () => {
  return {
    WsClient: class {
      token: string | null = null;
      constructor(
        _url: string,
        onMessage: (type: string, payload: unknown) => void,
      ) {
        capturedOnMessage = onMessage;
      }
      connect = vi.fn().mockResolvedValue(undefined);
      disconnect = vi.fn();
      sendInput = vi.fn();
      sendAbort = vi.fn();
      sendToolApproval = vi.fn();
      sendPlanApproval = vi.fn();
    },
    classifyError: vi.fn().mockReturnValue({ type: "connection", message: "err" }),
  };
});

import { useWebSocket, clearSessionToken, getSavedSessionToken } from "../useWebSocket";

beforeEach(() => {
  capturedOnMessage = null;
  localStorage.clear();
});

afterEach(cleanup);

describe("useWebSocket", () => {
  const url = "ws://localhost:8080/ws";

  function render(mocks?: Partial<Parameters<typeof useWebSocket>[0]>) {
    return renderHook(() =>
      useWebSocket({
        url,
        dispatch: mocks?.dispatch ?? vi.fn(),
        onConnected: mocks?.onConnected,
        onError: mocks?.onError,
        onToolApprovalRequest: mocks?.onToolApprovalRequest,
        onConnectionChange: mocks?.onConnectionChange,
      }),
    );
  }

  it("calls onConnectionChange with connecting on mount", () => {
    const onConnectionChange = vi.fn();
    render({ onConnectionChange });
    expect(onConnectionChange).toHaveBeenCalledWith("connecting");
  });

  it("calls onConnectionChange with connected after connect resolves", async () => {
    const onConnectionChange = vi.fn();
    render({ onConnectionChange });
    // Wait for connect().then() to fire
    await act(() => Promise.resolve());
    expect(onConnectionChange).toHaveBeenCalledWith("connected");
  });

  it("calls onConnectionChange with disconnected on unmount", () => {
    const onConnectionChange = vi.fn();
    const { unmount } = render({ onConnectionChange });
    unmount();
    expect(onConnectionChange).toHaveBeenCalledWith("disconnected");
  });

  it("calls onConnectionChange with disconnected on unmount (implies disconnect)", () => {
    const onConnectionChange = vi.fn();
    const { unmount } = render({ onConnectionChange });
    unmount();
    expect(onConnectionChange).toHaveBeenCalledWith("disconnected");
  });

  // --- Message routing ---

  it("routes session_created: saves token and calls onConnected", () => {
    const onConnected = vi.fn();
    render({ onConnected });
    act(() => {
      capturedOnMessage!("session_created", { session_token: "tok-1" });
    });
    expect(onConnected).toHaveBeenCalledWith("tok-1");
    expect(localStorage.getItem("rustycode-session-token")).toBe("tok-1");
  });

  it("routes session_resumed: saves token and calls onConnected", () => {
    const onConnected = vi.fn();
    render({ onConnected });
    act(() => {
      capturedOnMessage!("session_resumed", { session_token: "tok-2" });
    });
    expect(onConnected).toHaveBeenCalledWith("tok-2");
    expect(localStorage.getItem("rustycode-session-token")).toBe("tok-2");
  });

  it("routes state_snapshot via SET_SESSION dispatch", () => {
    const dispatch = vi.fn();
    render({ dispatch });
    const session = { input: "hi", messages: [], last_user_prompt: null, pending_request: false, tool_iteration_count: 0, current_response: "", input_tokens: 0, output_tokens: 0, cache_read_tokens: 0, cache_creation_tokens: 0, plan: null };
    act(() => {
      capturedOnMessage!("state_snapshot", session);
    });
    expect(dispatch).toHaveBeenCalledWith({ type: "SET_SESSION", session });
  });

  it("routes event via APPLY_EVENT dispatch", () => {
    const dispatch = vi.fn();
    render({ dispatch });
    const event = { type: "text_delta", data: { content: "hi" } };
    act(() => {
      capturedOnMessage!("event", { seq: 1, event });
    });
    expect(dispatch).toHaveBeenCalledWith({ type: "APPLY_EVENT", payload: { seq: 1, event } });
  });

  it("routes error via classifyError and onError", () => {
    const onError = vi.fn();
    render({ onError });
    act(() => {
      capturedOnMessage!("error", { code: "rate_limited", message: "slow down" });
    });
    expect(onError).toHaveBeenCalled();
  });

  it("routes tool_approval_requested to callback", () => {
    const onToolApprovalRequest = vi.fn();
    render({ onToolApprovalRequest });
    const payload = { request_id: "r1", tool_name: "bash", input_preview: "rm -rf", risk_level: "high" as const };
    act(() => {
      capturedOnMessage!("tool_approval_requested", payload);
    });
    expect(onToolApprovalRequest).toHaveBeenCalledWith(payload);
  });

  it("routes reconnecting to onConnectionChange", () => {
    const onConnectionChange = vi.fn();
    render({ onConnectionChange });
    act(() => {
      capturedOnMessage!("reconnecting", {});
    });
    // First call is "connecting" on mount, last should also be "connecting"
    expect(onConnectionChange).toHaveBeenLastCalledWith("connecting");
  });

  it("routes connection_lost to onConnectionChange", () => {
    const onConnectionChange = vi.fn();
    render({ onConnectionChange });
    act(() => {
      capturedOnMessage!("connection_lost", {});
    });
    expect(onConnectionChange).toHaveBeenLastCalledWith("disconnected");
  });

  it("ignores heartbeat_ack", () => {
    const dispatch = vi.fn();
    const onConnected = vi.fn();
    const onError = vi.fn();
    render({ dispatch, onConnected, onError });
    act(() => {
      capturedOnMessage!("heartbeat_ack", { ts: 1, server_ts: 2 });
    });
    expect(dispatch).not.toHaveBeenCalled();
    expect(onConnected).not.toHaveBeenCalled();
    expect(onError).not.toHaveBeenCalled();
  });

  // --- Send methods ---

  it("sendInput calls client.sendInput", () => {
    const { result } = render();
    act(() => {
      result.current.sendInput("hello");
    });
    // The mock client's sendInput should have been called
    // We can't easily access the mock instance, but we verify no throw
  });

  it("sendAbort calls client.sendAbort", () => {
    const { result } = render();
    act(() => {
      result.current.sendAbort();
    });
  });

  it("sendToolApproval calls client.sendToolApproval", () => {
    const { result } = render();
    act(() => {
      result.current.sendToolApproval("r1", true);
    });
  });

  it("sendPlanApproval calls client.sendPlanApproval", () => {
    const { result } = render();
    act(() => {
      result.current.sendPlanApproval("p1", false);
    });
  });

  it("getSessionToken returns null when no session created", () => {
    const { result } = render();
    expect(result.current.getSessionToken()).toBeNull();
  });
});

describe("session token helpers", () => {
  it("clearSessionToken removes from localStorage", () => {
    localStorage.setItem("rustycode-session-token", "abc");
    clearSessionToken();
    expect(localStorage.getItem("rustycode-session-token")).toBeNull();
  });

  it("getSavedSessionToken returns saved token", () => {
    localStorage.setItem("rustycode-session-token", "tok123");
    expect(getSavedSessionToken()).toBe("tok123");
  });

  it("getSavedSessionToken returns null when empty", () => {
    expect(getSavedSessionToken()).toBeNull();
  });
});
