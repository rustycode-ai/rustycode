import { describe, it, expect } from "vitest";
import { applyEvent } from "../event-reducer";
import type { FrontendSession, StreamEvent } from "../../protocol/types";

function emptySession(): FrontendSession {
  return {
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
}

function assistantSession(content = ""): FrontendSession {
  return {
    ...emptySession(),
    messages: [
      { id: "a1", content, kind: "Assistant", parts: [] },
    ],
  };
}

describe("applyEvent", () => {
  it("returns session unchanged for turn_started", () => {
    const session = emptySession();
    const result = applyEvent(session, { type: "turn_started", data: { turn: 1 } });
    expect(result).toEqual(session);
  });

  it("returns session unchanged for turn_completed", () => {
    const session = emptySession();
    const result = applyEvent(session, { type: "turn_completed", data: { stop_reason: "end_turn" } });
    expect(result).toEqual(session);
  });

  it("updates token_usage", () => {
    const session = emptySession();
    const result = applyEvent(session, {
      type: "token_usage",
      data: { input_tokens: 100, output_tokens: 50 },
    });
    expect(result.input_tokens).toBe(100);
    expect(result.output_tokens).toBe(50);
  });

  it("updates cache_usage", () => {
    const session = emptySession();
    const result = applyEvent(session, {
      type: "cache_usage",
      data: { cache_read_tokens: 200, cache_creation_tokens: 300 },
    });
    expect(result.cache_read_tokens).toBe(200);
    expect(result.cache_creation_tokens).toBe(300);
  });

  it("appends text_delta to last assistant message", () => {
    const session = assistantSession();
    const result = applyEvent(session, {
      type: "text_delta",
      data: { content: "Hello" },
    });
    expect(result.current_response).toBe("Hello");
    expect(result.messages[0].parts).toEqual([{ type: "text", content: "Hello" }]);
  });

  it("consecutive text_deltas merge into single text part", () => {
    let session = assistantSession();
    session = applyEvent(session, { type: "text_delta", data: { content: "Hel" } });
    session = applyEvent(session, { type: "text_delta", data: { content: "lo" } });
    expect(session.current_response).toBe("Hello");
    expect(session.messages[0].parts).toEqual([{ type: "text", content: "Hello" }]);
  });

  it("appends thinking_delta to last assistant message", () => {
    const session = assistantSession();
    const result = applyEvent(session, {
      type: "thinking_delta",
      data: { content: "hmm" },
    });
    expect(result.messages[0].parts).toEqual([{ type: "thinking", content: "hmm" }]);
  });

  it("starts a tool call and increments iteration count", () => {
    const session = assistantSession();
    const result = applyEvent(session, {
      type: "tool_call_started",
      data: { id: "t1", name: "bash" },
    });
    expect(result.tool_iteration_count).toBe(1);
    expect(result.messages[0].parts).toHaveLength(1);
    const part = result.messages[0].parts[0];
    expect(part.type).toBe("tool_call");
    if (part.type === "tool_call") {
      expect(part.id).toBe("t1");
      expect(part.name).toBe("bash");
      expect(part.status).toBe("pending");
    }
  });

  it("appends tool input chunks", () => {
    let session = assistantSession();
    session = applyEvent(session, { type: "tool_call_started", data: { id: "t1", name: "bash" } });
    session = applyEvent(session, { type: "tool_input_delta", data: { id: "t1", chunk: "ls " } });
    session = applyEvent(session, { type: "tool_input_delta", data: { id: "t1", chunk: "-la" } });
    const part = session.messages[0].parts[0];
    if (part.type === "tool_call") {
      expect(part.input).toBe("ls -la");
    }
  });

  it("completes a tool call with output", () => {
    let session = assistantSession();
    session = applyEvent(session, { type: "tool_call_started", data: { id: "t1", name: "bash" } });
    session = applyEvent(session, {
      type: "tool_exec_completed",
      data: { id: "t1", name: "bash", output: "file.txt", is_error: false },
    });
    const part = session.messages[0].parts[0];
    if (part.type === "tool_call") {
      expect(part.status).toBe("completed");
      expect(part.output).toBe("file.txt");
    }
  });

  it("completes a tool call with error status", () => {
    let session = assistantSession();
    session = applyEvent(session, { type: "tool_call_started", data: { id: "t1", name: "bash" } });
    session = applyEvent(session, {
      type: "tool_exec_completed",
      data: { id: "t1", name: "bash", output: "command failed", is_error: true },
    });
    const part = session.messages[0].parts[0];
    if (part.type === "tool_call") {
      expect(part.status).toBe("error");
    }
  });

  it("sets pending_request false on done", () => {
    const session = { ...emptySession(), pending_request: true };
    const result = applyEvent(session, { type: "done", data: {} });
    expect(result.pending_request).toBe(false);
  });

  it("creates plan on plan_created", () => {
    const session = emptySession();
    const result = applyEvent(session, {
      type: "plan_created",
      data: {
        id: "p1",
        title: "My Plan",
        steps: [
          { name: "Step 1", description: "Do thing" },
          { name: "Step 2", description: "Do other" },
        ],
      },
    });
    expect(result.plan).not.toBeNull();
    const plan = result.plan!;
    expect(plan.id).toBe("p1");
    expect(plan.title).toBe("My Plan");
    expect(plan.steps).toHaveLength(2);
    expect(plan.steps[0].status).toBe("pending");
    expect(plan.completed).toBe(false);
  });

  it("updates plan step on plan_step_started", () => {
    let session = emptySession();
    session = applyEvent(session, {
      type: "plan_created",
      data: { id: "p1", title: "P", steps: [{ name: "S1", description: "D1" }] },
    });
    session = applyEvent(session, {
      type: "plan_step_started",
      data: { plan_id: "p1", step_index: 0 },
    });
    expect(session.plan?.steps[0]?.status).toBe("running");
  });

  it("ignores plan events for wrong plan id", () => {
    let session = emptySession();
    session = applyEvent(session, {
      type: "plan_created",
      data: { id: "p1", title: "P", steps: [{ name: "S1", description: "D1" }] },
    });
    const result = applyEvent(session, {
      type: "plan_step_started",
      data: { plan_id: "p2", step_index: 0 },
    });
    expect(result.plan?.steps[0]?.status).toBe("pending");
  });

  it("marks step completed with success and message", () => {
    let session = emptySession();
    session = applyEvent(session, {
      type: "plan_created",
      data: { id: "p1", title: "P", steps: [{ name: "S1", description: "D1" }] },
    });
    session = applyEvent(session, {
      type: "plan_step_completed",
      data: { plan_id: "p1", step_index: 0, success: true, message: "done" },
    });
    expect(session.plan?.steps[0]?.status).toBe("completed");
    expect(session.plan?.steps[0]?.message).toBe("done");
  });

  it("marks step failed on plan_step_completed with success=false", () => {
    let session = emptySession();
    session = applyEvent(session, {
      type: "plan_created",
      data: { id: "p1", title: "P", steps: [{ name: "S1", description: "D1" }] },
    });
    session = applyEvent(session, {
      type: "plan_step_completed",
      data: { plan_id: "p1", step_index: 0, success: false, message: "error" },
    });
    expect(session.plan?.steps[0]?.status).toBe("failed");
  });

  it("completes plan on plan_completed", () => {
    let session = emptySession();
    session = applyEvent(session, {
      type: "plan_created",
      data: { id: "p1", title: "P", steps: [{ name: "S1", description: "D1" }] },
    });
    session = applyEvent(session, {
      type: "plan_completed",
      data: { plan_id: "p1", success: true, summary: "All good" },
    });
    expect(session.plan?.completed).toBe(true);
    expect(session.plan?.success).toBe(true);
    expect(session.plan?.summary).toBe("All good");
    expect(session.plan?.awaitingApproval).toBe(false);
  });

  it("ignores plan_completed for wrong plan id", () => {
    let session = emptySession();
    session = applyEvent(session, {
      type: "plan_created",
      data: { id: "p1", title: "P", steps: [{ name: "S1", description: "D1" }] },
    });
    const result = applyEvent(session, {
      type: "plan_completed",
      data: { plan_id: "p99", success: true, summary: "wrong" },
    });
    expect(result.plan?.completed).toBe(false);
  });

  it("sets awaitingApproval on plan_approval_requested", () => {
    const session = emptySession();
    const result = applyEvent(session, {
      type: "plan_approval_requested",
      data: { plan_id: "p1", title: "Approve", steps: [{ name: "S1", description: "D1" }] },
    });
    expect(result.plan?.awaitingApproval).toBe(true);
    expect(result.plan?.title).toBe("Approve");
  });

  it("text_delta is no-op when no assistant message exists", () => {
    const session = emptySession();
    const result = applyEvent(session, { type: "text_delta", data: { content: "hello" } });
    expect(result.messages).toHaveLength(0);
    expect(result.current_response).toBe("");
  });

  it("tool_call_started creates assistant message and increments counter when none exists", () => {
    const session = emptySession();
    const result = applyEvent(session, { type: "tool_call_started", data: { id: "t1", name: "bash" } });
    expect(result.tool_iteration_count).toBe(1);
    expect(result.messages).toHaveLength(1);
    expect(result.messages[0].kind).toBe("Assistant");
    expect(result.messages[0].parts).toHaveLength(1);
    expect(result.messages[0].parts[0].type).toBe("tool_call");
  });

  it("returns session for unknown event type", () => {
    const session = emptySession();
    const result = applyEvent(session, { type: "unknown_event" } as unknown as StreamEvent);
    expect(result).toEqual(session);
  });

  it("updates tool status to running on tool_exec_started", () => {
    let session = assistantSession();
    session = applyEvent(session, { type: "tool_call_started", data: { id: "t1", name: "bash" } });
    session = applyEvent(session, { type: "tool_exec_started", data: { id: "t1", name: "bash" } });
    const part = session.messages[0].parts[0];
    if (part.type === "tool_call") {
      expect(part.status).toBe("running");
    }
  });

  it("finalizes response on done when current_response is set", () => {
    let session = { ...assistantSession(), pending_request: true, current_response: "Hello" };
    session = applyEvent(session, {
      type: "text_delta",
      data: { content: "Hello" },
    });
    session = applyEvent(session, { type: "done", data: {} });
    expect(session.pending_request).toBe(false);
    expect(session.messages[0].parts).toEqual([{ type: "text", content: "Hello" }]);
  });

  it("plan_completed is no-op when no plan exists", () => {
    const session = emptySession();
    const result = applyEvent(session, {
      type: "plan_completed",
      data: { plan_id: "p1", success: true, summary: "done" },
    });
    expect(result.plan).toBeNull();
  });

  it("thinking then text creates separate parts", () => {
    let session = assistantSession();
    session = applyEvent(session, { type: "thinking_delta", data: { content: "hmm" } });
    session = applyEvent(session, { type: "text_delta", data: { content: "Hello" } });
    expect(session.messages[0].parts).toHaveLength(2);
    expect(session.messages[0].parts[0]).toEqual({ type: "thinking", content: "hmm" });
    expect(session.messages[0].parts[1]).toEqual({ type: "text", content: "Hello" });
  });

  it("consecutive thinking deltas merge", () => {
    let session = assistantSession();
    session = applyEvent(session, { type: "thinking_delta", data: { content: "let me" } });
    session = applyEvent(session, { type: "thinking_delta", data: { content: " think" } });
    expect(session.messages[0].parts).toEqual([{ type: "thinking", content: "let me think" }]);
  });

  it("interleaved tool calls create separate parts", () => {
    let session = assistantSession();
    session = applyEvent(session, { type: "text_delta", data: { content: "Running " } });
    session = applyEvent(session, { type: "tool_call_started", data: { id: "t1", name: "read" } });
    session = applyEvent(session, { type: "text_delta", data: { content: " and " } });
    session = applyEvent(session, { type: "tool_call_started", data: { id: "t2", name: "bash" } });
    expect(session.messages[0].parts).toHaveLength(4);
    expect(session.messages[0].parts[0]).toEqual({ type: "text", content: "Running " });
    expect(session.messages[0].parts[1].type).toBe("tool_call");
    expect(session.messages[0].parts[2]).toEqual({ type: "text", content: " and " });
    expect(session.messages[0].parts[3].type).toBe("tool_call");
    expect(session.tool_iteration_count).toBe(2);
  });

  it("tool_input_delta for unknown tool id is no-op", () => {
    let session = assistantSession();
    session = applyEvent(session, { type: "tool_call_started", data: { id: "t1", name: "bash" } });
    const before = session.messages[0].parts;
    session = applyEvent(session, { type: "tool_input_delta", data: { id: "unknown", chunk: "oops" } });
    expect(session.messages[0].parts).toEqual(before);
  });

  it("tool_exec_completed for unknown tool id is no-op", () => {
    let session = assistantSession();
    session = applyEvent(session, { type: "tool_call_started", data: { id: "t1", name: "bash" } });
    const before = session.messages[0].parts;
    session = applyEvent(session, {
      type: "tool_exec_completed",
      data: { id: "unknown", name: "bash", output: "x", is_error: false },
    });
    expect(session.messages[0].parts).toEqual(before);
  });

  it("plan with multiple steps — partial completion", () => {
    let session = emptySession();
    session = applyEvent(session, {
      type: "plan_created",
      data: {
        id: "p1",
        title: "Multi",
        steps: [
          { name: "S1", description: "D1" },
          { name: "S2", description: "D2" },
          { name: "S3", description: "D3" },
        ],
      },
    });
    session = applyEvent(session, { type: "plan_step_started", data: { plan_id: "p1", step_index: 0 } });
    session = applyEvent(session, { type: "plan_step_completed", data: { plan_id: "p1", step_index: 0, success: true, message: "ok" } });
    session = applyEvent(session, { type: "plan_step_started", data: { plan_id: "p1", step_index: 1 } });
    expect(session.plan!.steps[0].status).toBe("completed");
    expect(session.plan!.steps[1].status).toBe("running");
    expect(session.plan!.steps[2].status).toBe("pending");
    expect(session.plan!.completed).toBe(false);
  });

  it("plan_step_started with out-of-range index does not crash", () => {
    let session = emptySession();
    session = applyEvent(session, {
      type: "plan_created",
      data: { id: "p1", title: "P", steps: [{ name: "S1", description: "D1" }] },
    });
    // step_index 99 doesn't exist — map just won't match any element
    const result = applyEvent(session, { type: "plan_step_started", data: { plan_id: "p1", step_index: 99 } });
    expect(result.plan!.steps).toHaveLength(1);
    expect(result.plan!.steps[0].status).toBe("pending");
  });

  it("done clears pending_request even without current_response", () => {
    const session = { ...emptySession(), pending_request: true };
    const result = applyEvent(session, { type: "done", data: {} });
    expect(result.pending_request).toBe(false);
    expect(result.current_response).toBe("");
  });
});
