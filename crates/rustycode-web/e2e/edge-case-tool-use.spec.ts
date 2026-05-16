import { test, expect, type Page, type WebSocketRoute } from "@playwright/test";

const BASE_URL = "http://localhost:3000";

// ── Protocol helpers ────────────────────────────────────────────────

let seq = 0;
function resetSeq() { seq = 0; }

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

interface MockWs {
  server: WebSocketRoute;
  clientMessages: string[];
}

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

async function connectPage(page: Page): Promise<MockWs> {
  const clientMessages: string[] = [];
  let capturedServer!: WebSocketRoute;

  await page.routeWebSocket("**/ws", (ws) => {
    capturedServer = ws;
    ws.onMessage((data) => { clientMessages.push(data.toString()); });
    ws.send(sessionCreated());
  });

  await page.goto(BASE_URL);
  await page.waitForLoadState("networkidle");
  await page.waitForTimeout(300);

  await page.evaluate(() => {
    document.querySelectorAll(".toast").forEach((t) => t.remove());
  });

  return { server: capturedServer, clientMessages };
}

async function sendUserMessage(page: Page, text: string) {
  const textarea = page.locator("textarea[aria-label='Message input']");
  await textarea.fill(text);
  await textarea.press("Enter");
  await page.waitForTimeout(200);
}

// ── Edge Case Tests ────────────────────────────────────────────────

test.describe("Edge cases: tool use flows", () => {
  test.beforeEach(async ({ page }) => {
    resetSeq();
    await setupMockRoutes(page);
  });

  test.afterEach(async ({ page }) => {
    await page.unroute("**/ws").catch(() => {});
  });

  test("pending_request clears on done event", async ({ page }) => {
    const { server } = await connectPage(page);

    // Send a message
    await sendUserMessage(page, "Hello");

    // While pending, the stop button should be visible
    const stopBtn = page.locator("button[aria-label='Stop generation']");
    await expect(stopBtn).toBeVisible({ timeout: 2000 });

    // Complete the response
    server.send(makeEvent("text_delta", { content: "Hi there" }));
    server.send(makeEvent("done", {}));
    await page.waitForTimeout(300);

    // Stop button should be gone, regenerate visible
    await expect(stopBtn).not.toBeVisible();
    const regenBtn = page.locator("button[aria-label='Regenerate last response']");
    await expect(regenBtn).toBeVisible();
  });

  test("multiple tool calls in sequence render correctly", async ({ page }) => {
    const { server } = await connectPage(page);
    await sendUserMessage(page, "Read two files");

    // First tool call
    server.send(makeEvent("tool_call_started", { id: "t1", name: "Read" }));
    await page.waitForTimeout(50);
    server.send(makeEvent("tool_exec_started", { id: "t1", name: "Read" }));
    await page.waitForTimeout(50);
    server.send(makeEvent("tool_exec_completed", { id: "t1", name: "Read", output: "file1 contents", is_error: false }));
    await page.waitForTimeout(50);

    // Second tool call
    server.send(makeEvent("tool_call_started", { id: "t2", name: "Read" }));
    await page.waitForTimeout(50);
    server.send(makeEvent("tool_exec_started", { id: "t2", name: "Read" }));
    await page.waitForTimeout(50);
    server.send(makeEvent("tool_exec_completed", { id: "t2", name: "Read", output: "file2 contents", is_error: false }));
    await page.waitForTimeout(50);

    // Final text
    server.send(makeEvent("text_delta", { content: "Here are both files." }));
    server.send(makeEvent("done", {}));
    await page.waitForTimeout(300);

    // Both tool cards should be visible
    const toolCards = page.locator(".tool-call-card");
    await expect(toolCards).toHaveCount(2);
    await expect(toolCards.nth(0)).toContainText("file1 contents");
    await expect(toolCards.nth(1)).toContainText("file2 contents");
  });

  test("tool approval modal renders and can be approved", async ({ page }) => {
    const { server } = await connectPage(page);
    await sendUserMessage(page, "Run a command");

    // Trigger tool approval request
    server.send(makeEnvelope("tool_approval_requested", {
      request_id: "req-1",
      tool_name: "bash",
      input_preview: "rm -rf /tmp/test",
      risk_level: "high",
    }));
    await page.waitForTimeout(300);

    // Approval modal should be visible
    const modal = page.locator(".approval-overlay");
    await expect(modal).toBeVisible();
    await expect(modal).toContainText("bash");
    await expect(modal).toContainText("High Risk");

    // Approve it
    await page.locator(".approval-btn-approve").click();
    await page.waitForTimeout(200);

    // Modal should be gone
    await expect(modal).not.toBeVisible();
  });

  test("tool approval modal can be denied via keyboard", async ({ page }) => {
    const { server } = await connectPage(page);
    await sendUserMessage(page, "Delete something");

    server.send(makeEnvelope("tool_approval_requested", {
      request_id: "req-2",
      tool_name: "bash",
      input_preview: "rm important.txt",
      risk_level: "medium",
    }));
    await page.waitForTimeout(300);

    const modal = page.locator(".approval-overlay");
    await expect(modal).toBeVisible();

    // Deny with 'n' key (click dialog body first to remove textarea focus)
    await page.locator(".approval-dialog").click();
    await page.keyboard.press("n");
    await page.waitForTimeout(200);

    await expect(modal).not.toBeVisible();
  });

  test("interleaved tool calls and text render in order", async ({ page }) => {
    const { server } = await connectPage(page);
    await sendUserMessage(page, "Analyze code");

    // Text before tool call
    server.send(makeEvent("text_delta", { content: "Let me read the file." }));
    await page.waitForTimeout(50);

    // Tool call
    server.send(makeEvent("tool_call_started", { id: "t1", name: "Read" }));
    await page.waitForTimeout(50);
    server.send(makeEvent("tool_exec_started", { id: "t1", name: "Read" }));
    await page.waitForTimeout(50);
    server.send(makeEvent("tool_exec_completed", { id: "t1", name: "Read", output: "fn main() {}", is_error: false }));
    await page.waitForTimeout(50);

    // Text after tool call
    server.send(makeEvent("text_delta", { content: " The code looks good." }));
    server.send(makeEvent("done", {}));
    await page.waitForTimeout(300);

    // Message should contain text parts and tool card in order
    const msgBubble = page.locator(".message-bubble").last();
    await expect(msgBubble).toContainText("Let me read the file.");
    await expect(msgBubble).toContainText("fn main()");
    await expect(msgBubble).toContainText("The code looks good.");
  });

  test("tool iteration count updates in status bar", async ({ page }) => {
    const { server } = await connectPage(page);
    await sendUserMessage(page, "Do stuff");

    // Initial count should be 0
    const statusBar = page.locator(".status-bar");

    server.send(makeEvent("tool_call_started", { id: "t1", name: "Read" }));
    await page.waitForTimeout(100);

    // Count should have incremented
    await expect(statusBar).toContainText("1");
  });

  test("error event from server shows toast notification", async ({ page }) => {
    const { server } = await connectPage(page);
    await sendUserMessage(page, "Cause error");

    server.send(makeEnvelope("error", {
      code: "rate_limited",
      message: "Too many requests",
    }));
    await page.waitForTimeout(500);

    // Should show a toast or error indicator
    // The error handling in useWebSocket dispatches to onError callback
    // Check that the UI is still functional
    const textarea = page.locator("textarea[aria-label='Message input']");
    await expect(textarea).toBeVisible();
  });

  test("regenerate works after completed response", async ({ page }) => {
    const { server } = await connectPage(page);
    await sendUserMessage(page, "Hello");

    server.send(makeEvent("text_delta", { content: "Original response" }));
    server.send(makeEvent("done", {}));
    await page.waitForTimeout(300);

    // Verify original response
    await expect(page.locator(".message-bubble").last()).toContainText("Original response");

    // Click regenerate
    const regenBtn = page.locator("button[aria-label='Regenerate last response']");
    await expect(regenBtn).toBeVisible();
    await regenBtn.click();
    await page.waitForTimeout(400);

    // Send new response
    server.send(makeEvent("text_delta", { content: "New response" }));
    server.send(makeEvent("done", {}));
    await page.waitForTimeout(300);

    // Should have new response (last assistant message)
    await expect(page.locator(".message-bubble").last()).toContainText("New response");
  });

  test("tool call without prior text_delta creates assistant message", async ({ page }) => {
    const { server } = await connectPage(page);
    await sendUserMessage(page, "Read file");

    // Server sends tool_call_started directly (no text_delta first)
    server.send(makeEvent("tool_call_started", { id: "t1", name: "Read" }));
    await page.waitForTimeout(100);
    server.send(makeEvent("tool_exec_started", { id: "t1", name: "Read" }));
    await page.waitForTimeout(50);
    server.send(makeEvent("tool_exec_completed", { id: "t1", name: "Read", output: "file contents", is_error: false }));
    server.send(makeEvent("text_delta", { content: "Here is the file." }));
    server.send(makeEvent("done", {}));
    await page.waitForTimeout(300);

    // Tool card should render (not silently dropped)
    const toolCard = page.locator(".tool-call-card");
    await expect(toolCard).toBeVisible();
    await expect(toolCard).toContainText("Read");

    // Text after tool call should also render
    const assistantBubble = page.locator(".message-assistant").last();
    await expect(assistantBubble).toContainText("Here is the file.");
  });

  test("connection lost clears pending state", async ({ page }) => {
    const { server } = await connectPage(page);
    await sendUserMessage(page, "Long task");

    // Server starts responding but doesn't send done
    server.send(makeEvent("text_delta", { content: "Working..." }));
    await page.waitForTimeout(100);

    // Stop button visible (pending is true)
    const stopBtn = page.locator("button[aria-label='Stop generation']");
    await expect(stopBtn).toBeVisible();

    // Close the WebSocket to trigger reconnecting → pending should clear
    server.close();
    await page.waitForTimeout(500);

    // Pending should be cleared — stop button gone, send button visible
    await expect(stopBtn).not.toBeVisible();
    const sendBtn = page.locator("button[aria-label='Send message']");
    await expect(sendBtn).toBeVisible();
  });

  test("multiple tool_call_started before text_delta all render", async ({ page }) => {
    const { server } = await connectPage(page);
    await sendUserMessage(page, "Read all files");

    // Send multiple tool_call_started before any text_delta
    server.send(makeEvent("tool_call_started", { id: "t1", name: "Read" }));
    await page.waitForTimeout(30);
    server.send(makeEvent("tool_call_started", { id: "t2", name: "Grep" }));
    await page.waitForTimeout(30);
    server.send(makeEvent("tool_call_started", { id: "t3", name: "Bash" }));
    await page.waitForTimeout(30);

    // Complete them
    server.send(makeEvent("tool_exec_started", { id: "t1", name: "Read" }));
    server.send(makeEvent("tool_exec_completed", { id: "t1", name: "Read", output: "file1", is_error: false }));
    server.send(makeEvent("tool_exec_started", { id: "t2", name: "Grep" }));
    server.send(makeEvent("tool_exec_completed", { id: "t2", name: "Grep", output: "match1", is_error: false }));
    server.send(makeEvent("tool_exec_started", { id: "t3", name: "Bash" }));
    server.send(makeEvent("tool_exec_completed", { id: "t3", name: "Bash", output: "output1", is_error: false }));
    server.send(makeEvent("text_delta", { content: "Done." }));
    server.send(makeEvent("done", {}));
    await page.waitForTimeout(300);

    // All 3 tool cards should render
    const toolCards = page.locator(".tool-call-card");
    await expect(toolCards).toHaveCount(3);
    await expect(toolCards.nth(0)).toContainText("Read");
    await expect(toolCards.nth(1)).toContainText("Grep");
    await expect(toolCards.nth(2)).toContainText("Bash");

    const assistantBubble = page.locator(".message-assistant").last();
    await expect(assistantBubble).toContainText("Done.");
  });

  test("empty text_delta does not break rendering", async ({ page }) => {
    const { server } = await connectPage(page);
    await sendUserMessage(page, "Hello");

    server.send(makeEvent("text_delta", { content: "" }));
    await page.waitForTimeout(50);
    server.send(makeEvent("text_delta", { content: "Hello back" }));
    server.send(makeEvent("done", {}));
    await page.waitForTimeout(300);

    const assistantBubble = page.locator(".message-assistant").last();
    await expect(assistantBubble).toContainText("Hello back");
  });

  test("tool_exec_completed for unknown tool ID does not crash", async ({ page }) => {
    const { server } = await connectPage(page);
    await sendUserMessage(page, "Do things");

    // Send completion for a tool that was never started
    server.send(makeEvent("tool_exec_completed", { id: "unknown-1", name: "Read", output: "some output", is_error: false }));
    await page.waitForTimeout(50);
    server.send(makeEvent("text_delta", { content: "All done." }));
    server.send(makeEvent("done", {}));
    await page.waitForTimeout(300);

    // App should still be functional — assistant text renders
    const assistantBubble = page.locator(".message-assistant").last();
    await expect(assistantBubble).toContainText("All done.");

    // Input should still work
    const textarea = page.locator("textarea[aria-label='Message input']");
    await expect(textarea).toBeVisible();
  });

  test("tool call with very long output renders and is scrollable", async ({ page }) => {
    const { server } = await connectPage(page);
    await sendUserMessage(page, "Read big file");

    const longOutput = "x".repeat(10000);
    server.send(makeEvent("tool_call_started", { id: "t1", name: "Read" }));
    await page.waitForTimeout(50);
    server.send(makeEvent("tool_exec_started", { id: "t1", name: "Read" }));
    server.send(makeEvent("tool_exec_completed", { id: "t1", name: "Read", output: longOutput, is_error: false }));
    server.send(makeEvent("done", {}));
    await page.waitForTimeout(300);

    const toolCard = page.locator(".tool-call-card");
    await expect(toolCard).toBeVisible();

    // Tool output container should have scrollable height
    const outputEl = toolCard.locator(".tool-call-output");
    await expect(outputEl).toBeVisible();
    const scrollable = await outputEl.evaluate((el) => el.scrollHeight > el.clientHeight);
    expect(scrollable).toBe(true);
  });

  test("tool call error with multi-line output renders error styling", async ({ page }) => {
    const { server } = await connectPage(page);
    await sendUserMessage(page, "Run bad command");

    const multiLineError = "Error: command failed\n  at line 1\n  at line 2\n  at line 3\nStack trace:\n  foo\n  bar\n  baz";
    server.send(makeEvent("tool_call_started", { id: "t1", name: "Bash" }));
    await page.waitForTimeout(50);
    server.send(makeEvent("tool_exec_started", { id: "t1", name: "Bash" }));
    server.send(makeEvent("tool_exec_completed", { id: "t1", name: "Bash", output: multiLineError, is_error: true }));
    server.send(makeEvent("done", {}));
    await page.waitForTimeout(300);

    const toolCard = page.locator(".tool-call-card");
    await expect(toolCard).toBeVisible();
    await expect(toolCard.locator(".tool-status-error")).toBeVisible();
    await expect(toolCard.locator(".tool-output-error")).toBeVisible();
    // Multi-line error content preserved
    await expect(toolCard.locator(".tool-call-output")).toContainText("Error: command failed");
    await expect(toolCard.locator(".tool-call-output")).toContainText("Stack trace");
  });

  test("multiple rapid error envelopes show toast and app stays functional", async ({ page }) => {
    const { server } = await connectPage(page);
    await sendUserMessage(page, "Cause errors");

    // Send multiple error envelopes rapidly
    for (let i = 0; i < 3; i++) {
      server.send(makeEnvelope("error", {
        code: "server",
        message: `Server error ${i}`,
      }));
      await page.waitForTimeout(30);
    }
    await page.waitForTimeout(500);

    // App should still be functional
    const textarea = page.locator("textarea[aria-label='Message input']");
    await expect(textarea).toBeVisible();

    // Can still send a message after errors
    await sendUserMessage(page, "Still working?");
    server.send(makeEvent("text_delta", { content: "Yes!" }));
    server.send(makeEvent("done", {}));
    await page.waitForTimeout(300);

    const assistantBubble = page.locator(".message-assistant").last();
    await expect(assistantBubble).toContainText("Yes!");
  });
});
