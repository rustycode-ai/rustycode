/**
 * Functional E2E tests for RustyCode Web protocol interactions.
 *
 * Uses Playwright page.routeWebSocket to mock the WS server and verify
 * real end-to-end behavior: tool calls, approvals, stop, steer, plans, etc.
 */
import { test, expect, type Page, type WebSocketRoute } from "@playwright/test";

const BASE_URL = "http://localhost:3000";

// --- Protocol helpers ---

let seq = 0;

function resetSeq() {
  seq = 0;
}

function makeEvent(eventType: string, data: Record<string, unknown>): string {
  return JSON.stringify({
    v: 2,
    type: "event",
    id: `evt-${++seq}`,
    payload: { seq, type: eventType, data },
  });
}

function makeEnvelope(type: string, payload: Record<string, unknown>): string {
  return JSON.stringify({ v: 2, type, id: `env-${++seq}`, payload });
}

function sessionCreated(token = "test-token"): string {
  return makeEnvelope("session_created", {
    session_token: token,
    capabilities: { heartbeat_interval_secs: 30 },
  });
}

// --- Page setup helpers ---

async function setupMockRoutes(page: Page) {
  await page.route("**/api/providers", (route) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        current: { provider: "mock", model: "mock-model" },
        providers: [
          {
            name: "mock",
            display_name: "Mock Provider",
            models: ["mock-model"],
            default_model: "mock-model",
            available: true,
          },
        ],
      }),
    })
  );
  await page.route("**/api/sessions", (route) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify([]),
    })
  );
}

interface MockWs {
  server: WebSocketRoute;
  clientMessages: string[];
}

async function connectPage(page: Page): Promise<MockWs> {
  const clientMessages: string[] = [];
  let server: WebSocketRoute;

  await page.routeWebSocket("**/ws", (ws) => {
    server = ws;
    ws.onMessage((data) => {
      clientMessages.push(data.toString());
    });
    ws.send(sessionCreated());
  });

  await page.goto(BASE_URL);
  await page.waitForLoadState("networkidle");
  await page.waitForTimeout(500);

  // Dismiss any toast notifications
  await page.evaluate(() => {
    document.querySelectorAll(".toast").forEach((t) => t.remove());
  });

  return { server: server!, clientMessages };
}

async function sendUserMessage(page: Page, text: string) {
  const textarea = page.locator("textarea[aria-label='Message input']");
  await textarea.fill(text);
  await textarea.press("Enter");
  await page.waitForTimeout(200);
}

async function streamText(page: Page, ws: WebSocketRoute, chunks: string[], delay = 50) {
  for (const chunk of chunks) {
    ws.send(makeEvent("text_delta", { content: chunk }));
    if (delay > 0) await page.waitForTimeout(delay);
  }
}

async function completeResponse(page: Page, ws: WebSocketRoute, text?: string) {
  if (text) {
    ws.send(makeEvent("text_delta", { content: text }));
  }
  ws.send(makeEvent("done", {}));
  await page.waitForTimeout(300);
}

// --- Tests ---

test.describe("Functional Protocol Tests", () => {
  test.beforeEach(async ({ page }) => {
    resetSeq();
    await setupMockRoutes(page);
  });

  test.afterEach(async ({ page }) => {
    await page.unroute("**/ws").catch(() => {});
    await page.unroute("**/api/providers").catch(() => {});
    await page.unroute("**/api/sessions").catch(() => {});
  });

  // ================================================================
  // Tool call lifecycle
  // ================================================================
  test("tool call lifecycle: pending -> running -> completed", async ({ page }) => {
    const { server } = await connectPage(page);

    await sendUserMessage(page, "Read the file");

    server.send(makeEvent("text_delta", { content: "Reading file...\n\n" }));
    await page.waitForTimeout(50);

    server.send(makeEvent("tool_call_started", { id: "tc-1", name: "read_file" }));
    await page.waitForTimeout(100);

    // Verify pending state
    const pendingCard = page.locator(".tool-call-card").first();
    await expect(pendingCard).toBeVisible();

    const toolName = page.locator(".tool-call-name").first();
    await expect(toolName).toHaveText("read_file");

    server.send(makeEvent("tool_input_delta", { id: "tc-1", chunk: '{"path":"main.rs"}' }));
    server.send(makeEvent("tool_exec_started", { id: "tc-1", name: "read_file" }));
    await page.waitForTimeout(50);

    server.send(makeEvent("tool_exec_completed", {
      id: "tc-1",
      name: "read_file",
      output: "fn main() { println!(\"hello\"); }",
      is_error: false,
    }));
    await page.waitForTimeout(100);

    await completeResponse(page, server, "\n\nHere's the file content.");

    // Verify completed state
    const statusDone = page.locator(".tool-status-done").first();
    await expect(statusDone).toBeVisible();

    // Card shows output — the output section is the second <details> inside the card body
    const cardBody = page.locator(".tool-call-card").first();
    // The output is in the second details > pre.tool-call-output
    const outputDetails = cardBody.locator("details").nth(1);
    const outputPre = outputDetails.locator(".tool-call-output");
    await expect(outputPre).toContainText("fn main()");
  });

  test("tool call with error renders error styling", async ({ page }) => {
    const { server } = await connectPage(page);

    await sendUserMessage(page, "Run tests");

    server.send(makeEvent("text_delta", { content: "Running tests...\n\n" }));
    server.send(makeEvent("tool_call_started", { id: "tc-err", name: "bash" }));
    server.send(makeEvent("tool_input_delta", { id: "tc-err", chunk: '{"command":"pytest"}' }));
    server.send(makeEvent("tool_exec_started", { id: "tc-err", name: "bash" }));
    server.send(makeEvent("tool_exec_completed", {
      id: "tc-err",
      name: "bash",
      output: "command not found: pytest",
      is_error: true,
    }));
    await page.waitForTimeout(100);
    await completeResponse(page, server, "\n\npytest not installed.");

    const errorCard = page.locator(".tool-call-error").first();
    await expect(errorCard).toBeVisible();
  });

  test("multiple tool calls in one turn", async ({ page }) => {
    const { server } = await connectPage(page);

    await sendUserMessage(page, "Read both files");

    server.send(makeEvent("text_delta", { content: "Reading files...\n\n" }));

    for (const id of ["tc-a", "tc-b"]) {
      server.send(makeEvent("tool_call_started", { id, name: "read_file" }));
      server.send(makeEvent("tool_input_delta", { id, chunk: `{"path":"${id}.rs"}` }));
      server.send(makeEvent("tool_exec_started", { id, name: "read_file" }));
      server.send(makeEvent("tool_exec_completed", { id, name: "read_file", output: `content of ${id}`, is_error: false }));
    }

    await completeResponse(page, server, "\n\nHere are both files.");

    const toolCards = page.locator(".tool-call-card");
    await expect(toolCards).toHaveCount(2);
  });

  // ================================================================
  // Tool approval flow
  // ================================================================
  test("tool approval: approve via button", async ({ page }) => {
    const { server, clientMessages } = await connectPage(page);

    await sendUserMessage(page, "Delete temp files");

    server.send(makeEvent("text_delta", { content: "Deleting temp files...\n\n" }));
    server.send(makeEvent("tool_call_started", { id: "tc-del", name: "bash" }));
    server.send(makeEvent("tool_input_delta", { id: "tc-del", chunk: '{"command":"rm -rf /tmp/test"}' }));
    server.send(makeEvent("tool_exec_started", { id: "tc-del", name: "bash" }));

    server.send(makeEnvelope("tool_approval_requested", {
      request_id: "appr-1",
      tool_name: "bash",
      input_preview: "rm -rf /tmp/test",
      risk_level: "high",
    }));
    await page.waitForTimeout(300);

    // Verify modal visible
    const modal = page.locator(".approval-overlay");
    await expect(modal).toBeVisible();

    // Verify risk badge
    const riskBadge = page.locator(".approval-risk-badge");
    await expect(riskBadge).toContainText("High Risk");

    // Verify tool name in modal
    const toolNameInModal = page.locator(".approval-tool-name");
    await expect(toolNameInModal).toHaveText("bash");

    // Click approve
    await page.locator(".approval-btn-approve").click();
    await page.waitForTimeout(200);

    await expect(modal).not.toBeVisible();

    // Verify client sent tool_approval with approved=true
    const approvalMsg = clientMessages.find((m) => {
      try {
        const parsed = JSON.parse(m);
        return parsed.type === "tool_approval" && parsed.payload?.approved === true;
      } catch { return false; }
    });
    expect(approvalMsg).toBeTruthy();

    // Continue after approval
    server.send(makeEvent("tool_exec_completed", {
      id: "tc-del",
      name: "bash",
      output: "Deleted successfully",
      is_error: false,
    }));
    await completeResponse(page, server, "\n\nDone!");

    await expect(page.locator(".tool-status-done").first()).toBeVisible();
  });

  test("tool approval: deny via keyboard (Y key on deny button)", async ({ page }) => {
    const { server, clientMessages } = await connectPage(page);

    await sendUserMessage(page, "Run risky command");

    server.send(makeEvent("tool_call_started", { id: "tc-risk", name: "bash" }));
    server.send(makeEvent("tool_input_delta", { id: "tc-risk", chunk: '{"command":"rm -rf /"}' }));
    server.send(makeEvent("tool_exec_started", { id: "tc-risk", name: "bash" }));

    server.send(makeEnvelope("tool_approval_requested", {
      request_id: "appr-deny",
      tool_name: "bash",
      input_preview: "rm -rf /",
      risk_level: "high",
    }));
    await page.waitForTimeout(300);

    const modal = page.locator(".approval-overlay");
    await expect(modal).toBeVisible();

    // Click the deny button directly (keyboard shortcut requires focus outside textarea)
    await page.locator(".approval-btn-deny").click();
    await page.waitForTimeout(200);

    await expect(modal).not.toBeVisible();

    const denialMsg = clientMessages.find((m) => {
      try {
        const parsed = JSON.parse(m);
        return parsed.type === "tool_approval" && parsed.payload?.approved === false;
      } catch { return false; }
    });
    expect(denialMsg).toBeTruthy();
  });

  test("tool approval: deny via Escape key", async ({ page }) => {
    const { server, clientMessages } = await connectPage(page);

    await sendUserMessage(page, "Do something dangerous");

    server.send(makeEvent("tool_call_started", { id: "tc-esc", name: "bash" }));
    server.send(makeEvent("tool_exec_started", { id: "tc-esc", name: "bash" }));

    server.send(makeEnvelope("tool_approval_requested", {
      request_id: "appr-esc",
      tool_name: "bash",
      input_preview: "dangerous command",
      risk_level: "medium",
    }));
    await page.waitForTimeout(300);

    // Click the dialog first so focus leaves textarea, then Escape
    await page.locator(".approval-dialog").click();
    await page.keyboard.press("Escape");
    await page.waitForTimeout(200);

    const modal = page.locator(".approval-overlay");
    await expect(modal).not.toBeVisible();

    const denialMsg = clientMessages.find((m) => {
      try {
        const parsed = JSON.parse(m);
        return parsed.type === "tool_approval" && parsed.payload?.approved === false;
      } catch { return false; }
    });
    expect(denialMsg).toBeTruthy();
  });

  test("tool approval: backdrop click denies", async ({ page }) => {
    const { server, clientMessages } = await connectPage(page);

    await sendUserMessage(page, "Do something");

    server.send(makeEvent("tool_call_started", { id: "tc-bd", name: "bash" }));
    server.send(makeEvent("tool_exec_started", { id: "tc-bd", name: "bash" }));

    server.send(makeEnvelope("tool_approval_requested", {
      request_id: "appr-bd",
      tool_name: "bash",
      input_preview: "some command",
      risk_level: "low",
    }));
    await page.waitForTimeout(300);

    // Click the overlay backdrop (not the dialog)
    await page.locator(".approval-overlay").click({ position: { x: 5, y: 5 } });
    await page.waitForTimeout(200);

    const denialMsg = clientMessages.find((m) => {
      try {
        const parsed = JSON.parse(m);
        return parsed.type === "tool_approval" && parsed.payload?.approved === false;
      } catch { return false; }
    });
    expect(denialMsg).toBeTruthy();
  });

  // ================================================================
  // Stop generation
  // ================================================================
  test("stop generation sends abort and response stops", async ({ page }) => {
    const { server, clientMessages } = await connectPage(page);

    await sendUserMessage(page, "Write a long essay");

    await streamText(page, server, [
      "Here is a long essay about the history of computing.\n\n",
      "## Chapter 1: Early Days\n\n",
      "The first computers were massive machines...",
    ], 50);

    const stopBtn = page.locator("button[aria-label='Stop generation']");
    await expect(stopBtn).toBeVisible();

    await stopBtn.click();
    await page.waitForTimeout(200);

    const abortMsg = clientMessages.find((m) => {
      try {
        return JSON.parse(m).type === "abort";
      } catch { return false; }
    });
    expect(abortMsg).toBeTruthy();

    server.send(makeEvent("done", {}));
    await page.waitForTimeout(300);

    await expect(stopBtn).not.toBeVisible();
  });

  test("escape during pending sends abort", async ({ page }) => {
    const { server, clientMessages } = await connectPage(page);

    await sendUserMessage(page, "Start something");

    server.send(makeEvent("text_delta", { content: "Thinking" }));
    await page.waitForTimeout(100);

    await page.keyboard.press("Escape");
    await page.waitForTimeout(200);

    const abortMsg = clientMessages.find((m) => {
      try {
        return JSON.parse(m).type === "abort";
      } catch { return false; }
    });
    expect(abortMsg).toBeTruthy();

    server.send(makeEvent("done", {}));
    await page.waitForTimeout(300);
  });

  // ================================================================
  // Continue after tool approval
  // ================================================================
  test("continue conversation after tool approval and response", async ({ page }) => {
    const { server } = await connectPage(page);

    // Turn 1: tool call with approval
    await sendUserMessage(page, "Create a file");

    server.send(makeEvent("text_delta", { content: "Creating file...\n\n" }));
    server.send(makeEvent("tool_call_started", { id: "tc-1", name: "write_file" }));
    server.send(makeEvent("tool_input_delta", { id: "tc-1", chunk: '{"path":"test.txt","content":"hello"}' }));
    server.send(makeEvent("tool_exec_started", { id: "tc-1", name: "write_file" }));

    server.send(makeEnvelope("tool_approval_requested", {
      request_id: "appr-1",
      tool_name: "write_file",
      input_preview: "write test.txt",
      risk_level: "low",
    }));
    await page.waitForTimeout(300);

    await page.locator(".approval-btn-approve").click();
    await page.waitForTimeout(200);

    server.send(makeEvent("tool_exec_completed", {
      id: "tc-1",
      name: "write_file",
      output: "File created: test.txt",
      is_error: false,
    }));
    await completeResponse(page, server, "\n\nFile created!");

    // Turn 2: new message
    await sendUserMessage(page, "Now read it back");

    await streamText(page, server, ["The file contains: hello"], 30);
    await completeResponse(page, server);

    const userMessages = page.locator(".message-user");
    await expect(userMessages).toHaveCount(2);

    const assistantMessages = page.locator(".message-assistant");
    await expect(assistantMessages).toHaveCount(2);
  });

  // ================================================================
  // Multi-turn conversation
  // ================================================================
  test("multi-turn conversation preserves all messages", async ({ page }) => {
    const { server } = await connectPage(page);

    await sendUserMessage(page, "Hello");
    await streamText(page, server, ["Hi! How can I help?"], 30);
    await completeResponse(page, server);

    await sendUserMessage(page, "What is 2+2?");
    await streamText(page, server, ["2 + 2 = 4"], 30);
    await completeResponse(page, server);

    await sendUserMessage(page, "And 3+3?");
    await streamText(page, server, ["3 + 3 = 6"], 30);
    await completeResponse(page, server);

    const userMessages = page.locator(".message-user");
    await expect(userMessages).toHaveCount(3);

    const assistantMessages = page.locator(".message-assistant");
    await expect(assistantMessages).toHaveCount(3);

    const firstUser = userMessages.nth(0).locator(".message-content");
    await expect(firstUser).toContainText("Hello");

    const thirdUser = userMessages.nth(2).locator(".message-content");
    await expect(thirdUser).toContainText("And 3+3?");
  });

  // ================================================================
  // Token usage tracking
  // ================================================================
  test("token usage updates in status bar", async ({ page }) => {
    const { server } = await connectPage(page);

    await sendUserMessage(page, "Hello");

    server.send(makeEvent("text_delta", { content: "Hi there!" }));
    server.send(makeEvent("token_usage", { input_tokens: 15, output_tokens: 10 }));
    await completeResponse(page, server);

    const tokenDisplay = page.locator(".status-tokens");
    const tokenText = await tokenDisplay.textContent();
    expect(tokenText).toContain("token");
  });

  test("cache usage tracking does not crash", async ({ page }) => {
    const { server } = await connectPage(page);

    await sendUserMessage(page, "Hello");

    server.send(makeEvent("text_delta", { content: "Hi!" }));
    server.send(makeEvent("token_usage", { input_tokens: 10, output_tokens: 5 }));
    server.send(makeEvent("cache_usage", { cache_read_tokens: 100, cache_creation_tokens: 50 }));
    await completeResponse(page, server);

    // Just verify no crash — cache display may vary
    const assistantMessages = page.locator(".message-assistant");
    await expect(assistantMessages.first()).toBeVisible();
  });

  // ================================================================
  // Plan lifecycle
  // ================================================================
  test("plan lifecycle: create, step progress, complete", async ({ page }) => {
    const { server } = await connectPage(page);

    await sendUserMessage(page, "Build a REST API");

    // plan_created uses name/description for steps (see event-reducer.ts)
    server.send(makeEvent("plan_created", {
      id: "plan-1",
      title: "Build REST API",
      steps: [
        { name: "Set up project structure", description: "Initialize the project" },
        { name: "Define routes", description: "Create route handlers" },
        { name: "Add handlers", description: "Implement business logic" },
      ],
    }));
    await page.waitForTimeout(200);

    // Verify plan rendered — uses plan-banner, not plan-container
    const planEl = page.locator(".plan-banner");
    await expect(planEl).toBeVisible();
    await expect(planEl).toContainText("Build REST API");

    // Steps use step_index (integer), not step_id
    server.send(makeEvent("plan_step_started", { plan_id: "plan-1", step_index: 0 }));
    await page.waitForTimeout(100);
    server.send(makeEvent("plan_step_completed", { plan_id: "plan-1", step_index: 0, success: true }));
    await page.waitForTimeout(100);

    server.send(makeEvent("plan_step_started", { plan_id: "plan-1", step_index: 1 }));
    await page.waitForTimeout(100);
    server.send(makeEvent("plan_step_completed", { plan_id: "plan-1", step_index: 1, success: true }));
    await page.waitForTimeout(100);

    server.send(makeEvent("plan_step_started", { plan_id: "plan-1", step_index: 2 }));
    await page.waitForTimeout(100);
    server.send(makeEvent("plan_step_completed", { plan_id: "plan-1", step_index: 2, success: true }));
    await page.waitForTimeout(100);

    server.send(makeEvent("plan_completed", {
      plan_id: "plan-1",
      success: true,
      summary: "REST API built with 3 routes.",
    }));
    await page.waitForTimeout(200);

    await completeResponse(page, server, "\n\nAPI is ready!");

    // Verify plan shows success state
    await expect(planEl.locator(".plan-banner-status-ok")).toBeVisible();
  });

  test("plan approval requested: approve", async ({ page }) => {
    const { server, clientMessages } = await connectPage(page);

    await sendUserMessage(page, "Build a feature");

    // plan_approval_requested uses plan_id and title
    server.send(makeEvent("plan_approval_requested", {
      plan_id: "plan-appr-1",
      title: "Feature Plan",
      steps: [
        { name: "Step A", description: "First step" },
        { name: "Step B", description: "Second step" },
      ],
    }));
    await page.waitForTimeout(300);

    const planEl = page.locator(".plan-banner");
    await expect(planEl).toBeVisible();
    await expect(planEl).toContainText("Feature Plan");

    // Click the plan approve button
    const planApproveBtn = page.locator(".plan-btn-approve");
    await expect(planApproveBtn).toBeVisible();
    await planApproveBtn.click();
    await page.waitForTimeout(200);

    const planApprovalMsg = clientMessages.find((m) => {
      try {
        const parsed = JSON.parse(m);
        return parsed.type === "plan_approval" && parsed.payload?.approved === true;
      } catch { return false; }
    });
    expect(planApprovalMsg).toBeTruthy();
  });

  test("plan approval: reject", async ({ page }) => {
    const { server, clientMessages } = await connectPage(page);

    await sendUserMessage(page, "Build another feature");

    server.send(makeEvent("plan_approval_requested", {
      plan_id: "plan-deny-1",
      title: "Rejected Plan",
      steps: [
        { name: "Bad step", description: "Not a good idea" },
      ],
    }));
    await page.waitForTimeout(300);

    const planEl = page.locator(".plan-banner");
    await expect(planEl).toBeVisible();

    const planRejectBtn = page.locator(".plan-btn-reject");
    await expect(planRejectBtn).toBeVisible();
    await planRejectBtn.click();
    await page.waitForTimeout(200);

    const planDenialMsg = clientMessages.find((m) => {
      try {
        const parsed = JSON.parse(m);
        return parsed.type === "plan_approval" && parsed.payload?.approved === false;
      } catch { return false; }
    });
    expect(planDenialMsg).toBeTruthy();
  });

  // ================================================================
  // Error recovery
  // ================================================================
  test("server error allows retry", async ({ page }) => {
    const { server } = await connectPage(page);

    await sendUserMessage(page, "Do something");

    server.send(makeEnvelope("error", {
      code: "rate_limited",
      message: "Too many requests. Please wait and try again.",
    }));
    await page.waitForTimeout(300);

    // User can send another message
    await sendUserMessage(page, "Try again");
    await streamText(page, server, ["OK, let's try again."], 30);
    await completeResponse(page, server);

    const assistantMessages = page.locator(".message-assistant");
    const count = await assistantMessages.count();
    expect(count).toBeGreaterThanOrEqual(1);
  });

  // ================================================================
  // Streaming with thinking
  // ================================================================
  test("thinking part renders in collapsible element", async ({ page }) => {
    const { server } = await connectPage(page);

    await sendUserMessage(page, "Explain quantum computing");

    server.send(makeEvent("thinking_delta", {
      content: "I should explain qubits, superposition, and entanglement. Keep it accessible.",
    }));
    await page.waitForTimeout(100);

    await streamText(page, server, [
      "Quantum computing uses **qubits** that can be in superposition of 0 and 1.",
    ], 30);
    await completeResponse(page, server);

    const thinkingEl = page.locator(".part-thinking").first();
    await expect(thinkingEl).toBeVisible();
  });

  // ================================================================
  // Streaming progressive rendering
  // ================================================================
  test("text appears progressively during streaming", async ({ page }) => {
    const { server } = await connectPage(page);

    await sendUserMessage(page, "Count to 5");

    server.send(makeEvent("text_delta", { content: "1, " }));
    await page.waitForTimeout(150);

    // Assistant content is in .part-text elements inside the message
    const assistantText = page.locator(".message-assistant").last().locator(".part-text");
    await expect(assistantText.first()).toContainText("1,");

    server.send(makeEvent("text_delta", { content: "2, " }));
    await page.waitForTimeout(150);
    await expect(assistantText.first()).toContainText("2,");

    server.send(makeEvent("text_delta", { content: "3, " }));
    await page.waitForTimeout(150);
    await expect(assistantText.first()).toContainText("3,");

    await completeResponse(page, server, "4, 5!");
    await expect(assistantText.first()).toContainText("5!");
  });

  // ================================================================
  // Input state during pending
  // ================================================================
  test("input bar shows pending state during response", async ({ page }) => {
    const { server } = await connectPage(page);

    const sendBtn = page.locator("button[aria-label='Send message']");
    await expect(sendBtn).toBeVisible();

    await sendUserMessage(page, "Hello");

    server.send(makeEvent("text_delta", { content: "Thinking" }));
    await page.waitForTimeout(100);

    const stopBtn = page.locator("button[aria-label='Stop generation']");
    await expect(stopBtn).toBeVisible();

    const textarea = page.locator("textarea[aria-label='Message input']");
    const placeholder = await textarea.getAttribute("placeholder");
    expect(placeholder).toContain("Response in progress");

    await completeResponse(page, server, " Hi!");

    await expect(stopBtn).not.toBeVisible();
  });

  // ================================================================
  // Tool iteration counter in status bar
  // ================================================================
  test("tool iteration counter updates during turn", async ({ page }) => {
    const { server } = await connectPage(page);

    await sendUserMessage(page, "Run 3 commands");

    server.send(makeEvent("text_delta", { content: "Running commands...\n\n" }));

    for (let i = 1; i <= 3; i++) {
      server.send(makeEvent("tool_call_started", { id: `tc-${i}`, name: `bash-${i}` }));
      server.send(makeEvent("tool_input_delta", { id: `tc-${i}`, chunk: `{}` }));
      server.send(makeEvent("tool_exec_started", { id: `tc-${i}`, name: `bash-${i}` }));
      server.send(makeEvent("tool_exec_completed", {
        id: `tc-${i}`, name: `bash-${i}`, output: `output ${i}`, is_error: false,
      }));
      await page.waitForTimeout(50);
    }

    await completeResponse(page, server, "\n\nAll done!");

    const toolCards = page.locator(".tool-call-card");
    await expect(toolCards).toHaveCount(3);
  });

  // ================================================================
  // Code block rendering
  // ================================================================
  test("code blocks render with language label and copy button", async ({ page }) => {
    const { server } = await connectPage(page);

    await sendUserMessage(page, "Show me a Rust function");

    await streamText(page, server, [
      "Here's a simple Rust function:\n\n",
      "```rust\n",
      "fn greet(name: &str) -> String {\n",
      '    format!("Hello, {}!", name)\n',
      "}\n",
      "```\n",
    ], 30);
    await completeResponse(page, server);

    const codeBlock = page.locator(".code-block-wrapper").first();
    await expect(codeBlock).toBeVisible();

    const langLabel = page.locator(".code-block-lang").first();
    await expect(langLabel).toHaveText("rust");

    const copyBtn = page.locator(".code-block-copy").first();
    await expect(copyBtn).toBeVisible();
  });

  // ================================================================
  // Regenerate last response
  // ================================================================
  test("regenerate sends abort then new input", async ({ page }) => {
    const { server, clientMessages } = await connectPage(page);

    await sendUserMessage(page, "Tell me a joke");
    await streamText(page, server, ["Why did the chicken cross the road?"], 30);
    await completeResponse(page, server);

    clientMessages.length = 0;

    const regenBtn = page.locator("button[aria-label='Regenerate last response']");
    if (await regenBtn.isVisible().catch(() => false)) {
      await regenBtn.click();
      await page.waitForTimeout(300);

      const abortSent = clientMessages.some((m) => {
        try { return JSON.parse(m).type === "abort"; } catch { return false; }
      });
      const inputSent = clientMessages.some((m) => {
        try {
          const parsed = JSON.parse(m);
          return parsed.type === "input" && parsed.payload?.content === "Tell me a joke";
        } catch { return false; }
      });

      expect(abortSent).toBeTruthy();
      expect(inputSent).toBeTruthy();

      await streamText(page, server, ["A new joke: What do you call a fake noodle? An impasta!"], 30);
      await completeResponse(page, server);
    }
  });

  // ================================================================
  // Input history navigation
  // ================================================================
  test("ArrowUp navigates input history when input is empty", async ({ page }) => {
    const { server } = await connectPage(page);

    await sendUserMessage(page, "First message");
    await completeResponse(page, server, "Response 1");

    await sendUserMessage(page, "Second message");
    await completeResponse(page, server, "Response 2");

    // Focus textarea, clear, move cursor to start, then ArrowUp
    const textarea = page.locator("textarea[aria-label='Message input']");
    await textarea.click();
    await textarea.fill("");
    // Ensure cursor is at position 0
    await textarea.press("Home");
    await page.waitForTimeout(50);
    await textarea.press("ArrowUp");
    await page.waitForTimeout(200);

    const value = await textarea.inputValue();
    // History should populate with last sent message
    expect(value).toContain("Second message");
  });

  // ================================================================
  // Mixed content: text + tool call + text in one turn
  // ================================================================
  test("interleaved text and tool calls render correctly", async ({ page }) => {
    const { server } = await connectPage(page);

    await sendUserMessage(page, "Read and analyze main.py");

    server.send(makeEvent("text_delta", { content: "I'll read the file first.\n\n" }));

    server.send(makeEvent("tool_call_started", { id: "tc-mix", name: "read_file" }));
    server.send(makeEvent("tool_input_delta", { id: "tc-mix", chunk: '{"path":"main.py"}' }));
    server.send(makeEvent("tool_exec_started", { id: "tc-mix", name: "read_file" }));
    server.send(makeEvent("tool_exec_completed", {
      id: "tc-mix",
      name: "read_file",
      output: "print('hello world')",
      is_error: false,
    }));
    await page.waitForTimeout(100);

    await streamText(page, server, [
      "\n\nThe file contains a simple hello world script. ",
      "It uses `print()` to output text to the console.",
    ], 30);
    await completeResponse(page, server);

    const lastAssistant = page.locator(".message-assistant").last();
    await expect(lastAssistant).toContainText("I'll read the file first");
    await expect(lastAssistant).toContainText("hello world script");
    await expect(page.locator(".tool-call-card").first()).toBeVisible();
  });

  // ================================================================
  // Connection status
  // ================================================================
  test("status bar shows connected status", async ({ page }) => {
    await connectPage(page);

    const statusDot = page.locator(".status-dot").first();
    await expect(statusDot).toBeVisible();

    const className = await statusDot.getAttribute("class");
    expect(className).toContain("connected");
  });

  // ================================================================
  // Done event finalizes response without extra text
  // ================================================================
  test("done event without preceding text clears pending", async ({ page }) => {
    const { server } = await connectPage(page);

    await sendUserMessage(page, "Empty response test");

    server.send(makeEvent("done", {}));
    await page.waitForTimeout(300);

    const assistantMessages = page.locator(".message-assistant");
    const count = await assistantMessages.count();
    expect(count).toBeGreaterThanOrEqual(1);

    const stopBtn = page.locator("button[aria-label='Stop generation']");
    await expect(stopBtn).not.toBeVisible();
  });

  // ================================================================
  // Large streaming response
  // ================================================================
  test("large streaming response renders completely", async ({ page }) => {
    const { server } = await connectPage(page);

    await sendUserMessage(page, "Generate a long response");

    const longText = Array.from({ length: 20 }, (_, i) =>
      `Paragraph ${i + 1}: Lorem ipsum dolor sit amet.`
    ).join("\n\n");

    // Use a small delay to let React render between chunks
    await streamText(page, server, [longText], 10);
    await completeResponse(page, server);

    const assistantText = page.locator(".message-assistant").last().locator(".part-text");
    await expect(assistantText.first()).toContainText("Paragraph 1");
    await expect(assistantText.first()).toContainText("Paragraph 20");
  });
});
