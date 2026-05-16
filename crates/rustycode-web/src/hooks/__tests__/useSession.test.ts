import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { renderHook, act, cleanup } from "@testing-library/react";

// Mock useWebSocket to capture dispatch and verify calls
const mockSendInput = vi.fn();
const mockSendAbort = vi.fn();
const mockSendToolApproval = vi.fn();
const mockSendPlanApproval = vi.fn();
const mockGetSessionToken = vi.fn().mockReturnValue(null);

let capturedOptions: {
  dispatch: React.Dispatch<unknown>;
  onToolApprovalRequest?: (request: unknown) => void;
  onConnectionChange?: (status: string) => void;
} | null = null;

vi.mock("../useWebSocket", () => ({
  useWebSocket: (opts: Record<string, unknown>) => {
    capturedOptions = opts as typeof capturedOptions;
    return {
      sendInput: mockSendInput,
      sendAbort: mockSendAbort,
      sendToolApproval: mockSendToolApproval,
      sendPlanApproval: mockSendPlanApproval,
      getSessionToken: mockGetSessionToken,
    };
  },
}));

import { useSessionProvider } from "../useSession";

beforeEach(() => {
  vi.clearAllMocks();
  capturedOptions = null;
});

afterEach(cleanup);

describe("useSessionProvider", () => {
  it("initializes with empty session state", () => {
    const { result } = renderHook(() => useSessionProvider());
    const { state } = result.current;
    expect(state.input).toBe("");
    expect(state.messages).toEqual([]);
    expect(state.pending_request).toBe(false);
    expect(state.current_response).toBe("");
    expect(state.plan).toBeNull();
  });

  it("initializes with connecting status", () => {
    const { result } = renderHook(() => useSessionProvider());
    expect(result.current.connectionStatus).toBe("connecting");
  });

  it("initializes with no pending approval", () => {
    const { result } = renderHook(() => useSessionProvider());
    expect(result.current.pendingApproval).toBeNull();
  });

  // --- sendInput flow ---

  it("sendInput dispatches ADD_USER_MESSAGE, CLEAR_INPUT, SET_PENDING", () => {
    const { result } = renderHook(() => useSessionProvider());
    act(() => {
      result.current.contextValue.sendInput("hello world");
    });
    expect(mockSendInput).toHaveBeenCalledWith("hello world");
    const { state } = result.current;
    expect(state.pending_request).toBe(true);
    expect(state.input).toBe("");
    // Should have added a User message and an empty Assistant message
    expect(state.messages.length).toBeGreaterThanOrEqual(2);
    expect(state.messages[0].kind).toBe("User");
    expect(state.messages[1].kind).toBe("Assistant");
  });

  // --- Tool approval flow ---

  it("stores tool approval request when received", () => {
    const { result } = renderHook(() => useSessionProvider());
    const request = { request_id: "r1", tool_name: "bash", input_preview: "rm -rf /", risk_level: "high" as const };
    act(() => {
      capturedOptions!.onToolApprovalRequest!(request);
    });
    expect(result.current.pendingApproval).toEqual(request);
  });

  it("clears pending approval after approval response", () => {
    const { result } = renderHook(() => useSessionProvider());
    const request = { request_id: "r1", tool_name: "bash", input_preview: "ls", risk_level: "low" as const };
    act(() => {
      capturedOptions!.onToolApprovalRequest!(request);
    });
    expect(result.current.pendingApproval).not.toBeNull();
    act(() => {
      result.current.handleToolApprovalResponse("r1", true);
    });
    expect(result.current.pendingApproval).toBeNull();
    expect(mockSendToolApproval).toHaveBeenCalledWith("r1", true);
  });

  it("sends denial and clears pending approval", () => {
    const { result } = renderHook(() => useSessionProvider());
    const request = { request_id: "r2", tool_name: "bash", input_preview: "rm", risk_level: "high" as const };
    act(() => {
      capturedOptions!.onToolApprovalRequest!(request);
    });
    act(() => {
      result.current.handleToolApprovalResponse("r2", false);
    });
    expect(result.current.pendingApproval).toBeNull();
    expect(mockSendToolApproval).toHaveBeenCalledWith("r2", false);
  });

  // --- Connection status ---

  it("updates connection status via onConnectionChange", () => {
    const { result } = renderHook(() => useSessionProvider());
    act(() => {
      capturedOptions!.onConnectionChange!("connected");
    });
    expect(result.current.connectionStatus).toBe("connected");
  });

  it("tracks disconnected status", () => {
    const { result } = renderHook(() => useSessionProvider());
    act(() => {
      capturedOptions!.onConnectionChange!("connected");
    });
    act(() => {
      capturedOptions!.onConnectionChange!("disconnected");
    });
    expect(result.current.connectionStatus).toBe("disconnected");
  });

  // --- sendAbort ---

  it("exposes sendAbort from useWebSocket", () => {
    const { result } = renderHook(() => useSessionProvider());
    act(() => {
      result.current.sendAbort();
    });
    expect(mockSendAbort).toHaveBeenCalled();
  });

  // --- State updates via dispatch ---

  it("applies state updates via dispatch", () => {
    const { result } = renderHook(() => useSessionProvider());
    act(() => {
      capturedOptions!.dispatch({
        type: "SET_INPUT",
        input: "test input",
      });
    });
    expect(result.current.state.input).toBe("test input");
  });

  it("applies SET_PENDING via dispatch", () => {
    const { result } = renderHook(() => useSessionProvider());
    act(() => {
      capturedOptions!.dispatch({ type: "SET_PENDING", pending: true });
    });
    expect(result.current.state.pending_request).toBe(true);
  });
});
