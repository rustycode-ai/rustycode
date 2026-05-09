import { test, expect, type Page, type WebSocketRoute } from "@playwright/test";

const META = process.platform === "darwin" ? "Meta" : "Control";
const BASE_URL = "http://localhost:3000";

// ── Protocol helpers (matching functional-protocol.spec.ts) ────────

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

  // Dismiss toasts
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

async function streamText(page: Page, ws: WebSocketRoute, chunks: string[], delay = 50) {
  for (const chunk of chunks) {
    ws.send(makeEvent("text_delta", { content: chunk }));
    if (delay > 0) await page.waitForTimeout(delay);
  }
}

async function completeResponse(page: Page, ws: WebSocketRoute, text?: string) {
  if (text) ws.send(makeEvent("text_delta", { content: text }));
  ws.send(makeEvent("done", {}));
  await page.waitForTimeout(300);
}

// ── Page mock setup ────────────────────────────────────────────────

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

// ── US-001: App Load, Layout, Navigation ──────────────────────────

test.describe("US-001: App load, layout, navigation, accessibility", () => {
  test.beforeEach(async ({ page }) => {
    resetSeq();
    await setupMockRoutes(page);
  });

  test.afterEach(async ({ page }) => {
    await page.unroute("**/ws").catch(() => {});
  });

  test("status bar renders", async ({ page }) => {
    await connectPage(page);
    const statusBar = page.locator(".status-bar");
    await expect(statusBar).toBeVisible();
  });

  test("sidebar opens and closes with Cmd+B", async ({ page }) => {
    await connectPage(page);
    const sidebar = page.locator(".session-sidebar");
    // Sidebar starts visible (useState(true) in App.tsx)
    await expect(sidebar).toBeVisible();
    // Close it
    await page.keyboard.press(`${META}+b`);
    await expect(sidebar).not.toBeVisible();
    // Re-open
    await page.keyboard.press(`${META}+b`);
    await expect(sidebar).toBeVisible();
  });

  test("command palette opens with Cmd+K", async ({ page }) => {
    await connectPage(page);
    await page.keyboard.press(`${META}+k`);
    const palette = page.locator(".palette-overlay");
    await expect(palette).toBeVisible();
    await page.keyboard.press("Escape");
  });

  test("shortcuts overlay opens with Cmd+/", async ({ page }) => {
    await connectPage(page);
    await page.keyboard.press(`${META}+/`);
    const overlay = page.locator(".shortcuts-overlay");
    await expect(overlay).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(overlay).not.toBeVisible();
  });

  test("search overlay opens with Cmd+F", async ({ page }) => {
    await connectPage(page);
    await page.keyboard.press(`${META}+f`);
    const search = page.locator(".search-overlay");
    await expect(search).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(search).not.toBeVisible();
  });

  test("skip-to-content link exists for accessibility", async ({ page }) => {
    await connectPage(page);
    const skipLink = page.locator(".skip-link");
    await expect(skipLink).toBeAttached();
    expect(await skipLink.getAttribute("href")).toBe("#main-content");
  });

  test("no horizontal overflow at 1280px", async ({ page }) => {
    await connectPage(page);
    await page.setViewportSize({ width: 1280, height: 800 });
    await page.waitForTimeout(300);
    const scrollWidth = await page.evaluate(() => document.documentElement.scrollWidth);
    const clientWidth = await page.evaluate(() => document.documentElement.clientWidth);
    expect(scrollWidth).toBeLessThanOrEqual(clientWidth + 1);
  });

  test("no horizontal overflow at 375px", async ({ page }) => {
    await connectPage(page);
    await page.setViewportSize({ width: 375, height: 812 });
    await page.waitForTimeout(300);
    const scrollWidth = await page.evaluate(() => document.documentElement.scrollWidth);
    const clientWidth = await page.evaluate(() => document.documentElement.clientWidth);
    expect(scrollWidth).toBeLessThanOrEqual(clientWidth + 1);
  });
});

// ── US-002: Conversation Turns with WebSocket Mock ────────────────

test.describe("US-002: Conversation turns", () => {
  test.beforeEach(async ({ page }) => {
    resetSeq();
    await setupMockRoutes(page);
  });

  test.afterEach(async ({ page }) => {
    await page.unroute("**/ws").catch(() => {});
  });

  test("user message sends and renders", async ({ page }) => {
    const { server } = await connectPage(page);
    await sendUserMessage(page, "Hello RustyCode!");

    const userBubble = page.locator(".message-user");
    await expect(userBubble).toBeVisible();
    await expect(userBubble).toContainText("Hello RustyCode!");
  });

  test("streaming assistant text renders progressively", async ({ page }) => {
    const { server } = await connectPage(page);
    await sendUserMessage(page, "Explain Rust");

    await streamText(page, server, ["Rust is ", "a systems ", "programming language."], 80);
    await completeResponse(page, server);

    const assistantBubble = page.locator(".message-assistant");
    await expect(assistantBubble).toBeVisible();
    await expect(assistantBubble).toContainText("Rust is a systems programming language.");
  });

  test("multi-turn conversation works", async ({ page }) => {
    const { server } = await connectPage(page);

    // Turn 1
    await sendUserMessage(page, "What is Rust?");
    await streamText(page, server, ["A language."], 60);
    await completeResponse(page, server);

    // Turn 2
    await sendUserMessage(page, "And Go?");
    await streamText(page, server, ["Also a language."], 60);
    await completeResponse(page, server);

    // 2 user + 2 assistant
    const messages = page.locator(".message-bubble");
    await expect(messages).toHaveCount(4);
  });

  test("abort button appears during pending state", async ({ page }) => {
    const { server } = await connectPage(page);
    await sendUserMessage(page, "Long response");

    // Send text but NOT done — pending state
    server.send(makeEvent("text_delta", { content: "Thinking..." }));
    await page.waitForTimeout(100);

    const abortBtn = page.locator("button[aria-label='Stop generation']");
    await expect(abortBtn).toBeVisible();

    // Clean up
    server.send(makeEvent("done", {}));
    await page.waitForTimeout(200);
  });

  test("regenerate button visible after response", async ({ page }) => {
    const { server } = await connectPage(page);
    await sendUserMessage(page, "Hello");
    await streamText(page, server, ["Hi there!"], 60);
    await completeResponse(page, server);

    const regenBtn = page.locator("button[aria-label='Regenerate last response']");
    await expect(regenBtn).toBeVisible();
  });

  test("messages ordered newest at bottom", async ({ page }) => {
    const { server } = await connectPage(page);

    await sendUserMessage(page, "First");
    await streamText(page, server, ["Response 1"], 60);
    await completeResponse(page, server);

    await sendUserMessage(page, "Second");
    await streamText(page, server, ["Response 2"], 60);
    await completeResponse(page, server);

    const allText = await page.locator(".message-bubble").allTextContents();
    const firstIdx = allText.findIndex((t) => t.includes("First"));
    const secondIdx = allText.findIndex((t) => t.includes("Second"));
    expect(firstIdx).toBeLessThan(secondIdx);
  });
});

// ── US-003: Tool Calls, Thinking Parts, Error States ──────────────

test.describe("US-003: Tool calls, thinking, error states", () => {
  test.beforeEach(async ({ page }) => {
    resetSeq();
    await setupMockRoutes(page);
  });

  test.afterEach(async ({ page }) => {
    await page.unroute("**/ws").catch(() => {});
  });

  test("tool call renders pending state", async ({ page }) => {
    const { server } = await connectPage(page);
    await sendUserMessage(page, "Read a file");

    server.send(makeEvent("tool_call_started", { id: "tool-1", name: "read_file" }));
    server.send(makeEvent("tool_input_delta", { id: "tool-1", chunk: '{"path":"/src/main.rs"}' }));
    await page.waitForTimeout(200);

    const toolCard = page.locator(".tool-call-card");
    await expect(toolCard).toBeVisible();
    await expect(toolCard).toContainText("read_file");

    server.send(makeEvent("done", {}));
    await page.waitForTimeout(200);
  });

  test("tool call transitions to completed with output", async ({ page }) => {
    const { server } = await connectPage(page);
    await sendUserMessage(page, "Read file");

    server.send(makeEvent("tool_call_started", { id: "t1", name: "read_file" }));
    await page.waitForTimeout(50);
    server.send(makeEvent("tool_exec_started", { id: "t1", name: "read_file" }));
    await page.waitForTimeout(50);
    server.send(makeEvent("tool_exec_completed", { id: "t1", name: "read_file", output: "fn main() {}", is_error: false }));
    await page.waitForTimeout(50);
    server.send(makeEvent("text_delta", { content: "Here's the file." }));
    server.send(makeEvent("done", {}));
    await page.waitForTimeout(300);

    const toolCard = page.locator(".tool-call-card");
    await expect(toolCard).toBeVisible();
    await expect(toolCard).toContainText("fn main()");
  });

  test("tool call error state renders error styling", async ({ page }) => {
    const { server } = await connectPage(page);
    await sendUserMessage(page, "Bad command");

    server.send(makeEvent("tool_call_started", { id: "t2", name: "bash" }));
    await page.waitForTimeout(50);
    server.send(makeEvent("tool_exec_started", { id: "t2", name: "bash" }));
    await page.waitForTimeout(50);
    server.send(makeEvent("tool_exec_completed", { id: "t2", name: "bash", output: "command not found: badcmd", is_error: true }));
    server.send(makeEvent("done", {}));
    await page.waitForTimeout(300);

    const toolCard = page.locator(".tool-call-card");
    await expect(toolCard).toBeVisible();
    await expect(toolCard.locator(".tool-status-error")).toBeVisible();
    await expect(toolCard).toContainText("command not found");
  });

  test("thinking part renders in collapsible details", async ({ page }) => {
    const { server } = await connectPage(page);
    await sendUserMessage(page, "Think about this");

    server.send(makeEvent("thinking_delta", { content: "Let me consider the problem carefully..." }));
    await page.waitForTimeout(80);
    server.send(makeEvent("text_delta", { content: "Here is my answer." }));
    server.send(makeEvent("done", {}));
    await page.waitForTimeout(300);

    const thinkingPart = page.locator("details.part-thinking");
    await expect(thinkingPart).toBeVisible();
    await expect(thinkingPart.locator("summary")).toContainText(/Thinking/i);
  });

  test("code blocks render with syntax highlighting and copy", async ({ page }) => {
    const { server } = await connectPage(page);
    await sendUserMessage(page, "Show me code");

    server.send(makeEvent("text_delta", { content: "```rust\nfn main() {\n    println!(\"hello\");\n}\n```" }));
    server.send(makeEvent("done", {}));
    await page.waitForTimeout(300);

    const codeBlock = page.locator(".code-block-wrapper");
    await expect(codeBlock).toBeVisible();
    await expect(codeBlock.locator(".code-block-lang")).toContainText("rust");
    await expect(codeBlock.locator(".code-block-copy")).toBeVisible();
  });

  test("plan banner renders with approval buttons", async ({ page }) => {
    const { server } = await connectPage(page);
    await sendUserMessage(page, "Build this");

    server.send(makeEvent("plan_approval_requested", {
      plan_id: "plan-1",
      title: "Implementation Plan",
      steps: [
        { name: "Step 1", description: "Create files" },
        { name: "Step 2", description: "Write tests" },
      ],
    }));
    await page.waitForTimeout(300);

    const planBanner = page.locator(".plan-banner");
    await expect(planBanner).toBeVisible();
    await expect(planBanner).toContainText("Implementation Plan");
    await expect(planBanner.locator("button").first()).toBeVisible();
  });
});
