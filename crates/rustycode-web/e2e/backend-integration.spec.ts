/**
 * Backend Integration Tests — RustyCode Web
 *
 * Tests against the REAL RustyCode backend (axum serve).
 * Focus: error cases, edge cases, security boundaries, failure modes.
 *
 * Prerequisites: cargo run -p rustycode-cli -- serve --port 3001 --verbose
 * Run:           npx playwright test e2e/backend-integration.spec.ts --reporter=list
 */

import { test, expect } from "@playwright/test";

const BASE = "http://localhost:3001";
const FRONTEND = "http://localhost:3000";

// ============================================================
// 1. HTTP METHOD ENFORCEMENT
// ============================================================

test.describe("HTTP method enforcement", () => {
  // /api/health only accepts GET
  for (const method of ["POST", "PUT", "DELETE", "PATCH"] as const) {
    test(`${method} /api/health → 405`, async ({ request }) => {
      const r = await request.fetch(`${BASE}/api/health`, { method });
      expect(r.status()).toBe(405);
    });
  }

  // /call only accepts POST
  for (const method of ["GET", "PUT", "DELETE", "PATCH", "OPTIONS"] as const) {
    test(`${method} /call → 405`, async ({ request }) => {
      const r = await request.fetch(`${BASE}/call`, {
        method,
        data: method === "PUT" || method === "PATCH" ? {} : undefined,
      });
      expect(r.status()).toBe(405);
    });
  }

  // /ws only accepts GET (WebSocket upgrade)
  test("POST /ws → 405", async ({ request }) => {
    const r = await request.post(`${BASE}/ws`, { data: {} });
    expect(r.status()).toBe(405);
  });
});

// ============================================================
// 2. POST /call — INVALID PAYLOADS
// ============================================================

test.describe("POST /call invalid payloads", () => {
  test("empty body → missing field error", async ({ request }) => {
    const r = await request.post(`${BASE}/call`, { data: {} });
    expect(r.status()).toBe(200);
    const b = await r.json();
    expect(b).toHaveProperty("error");
    expect(b.error).toContain("missing field");
  });

  test("missing call_id → error", async ({ request }) => {
    const r = await request.post(`${BASE}/call`, {
      data: { name: "read_file", arguments: {} },
    });
    expect(r.status()).toBe(200);
    const b = await r.json();
    expect(b.error).toContain("missing field");
    expect(b.error).toContain("call_id");
  });

  test("missing name → error", async ({ request }) => {
    const r = await request.post(`${BASE}/call`, {
      data: { call_id: "t1", arguments: {} },
    });
    expect(r.status()).toBe(200);
    const b = await r.json();
    expect(b.error).toContain("missing field");
    expect(b.error).toContain("name");
  });

  test("missing arguments → error", async ({ request }) => {
    const r = await request.post(`${BASE}/call`, {
      data: { call_id: "t2", name: "read_file" },
    });
    expect(r.status()).toBe(200);
    const b = await r.json();
    expect(b.error).toContain("missing field");
    expect(b.error).toContain("arguments");
  });

  test("null arguments → error", async ({ request }) => {
    const r = await request.post(`${BASE}/call`, {
      data: { call_id: "t3", name: "read_file", arguments: null },
    });
    expect(r.status()).toBe(200);
    expect((await r.json())).toHaveProperty("error");
  });

  test("array arguments instead of object → error", async ({ request }) => {
    const r = await request.post(`${BASE}/call`, {
      data: { call_id: "t4", name: "read_file", arguments: ["/etc/hosts"] },
    });
    expect(r.status()).toBe(200);
    expect((await r.json())).toHaveProperty("error");
  });

  test("nonexistent tool name → failure result with error", async ({ request }) => {
    const r = await request.post(`${BASE}/call`, {
      data: { call_id: "t5", name: "totally_fake_tool_xyz", arguments: {} },
    });
    expect(r.status()).toBe(200);
    const b = await r.json();
    expect(b.success).toBe(false);
    expect(b).toHaveProperty("error");
    expect(b.call_id).toBe("t5");
  });

  test("extremely long call_id (10k chars) → server doesn't crash", async ({ request }) => {
    const r = await request.post(`${BASE}/call`, {
      data: { call_id: "x".repeat(10000), name: "read_file", arguments: { path: "/tmp" } },
    });
    // Should process without crashing (returns 200 with error or success)
    expect([200, 400, 413]).toContain(r.status());
  });

  test("unicode/special chars in arguments → handled safely", async ({ request }) => {
    const r = await request.post(`${BASE}/call`, {
      data: { call_id: "u1", name: "read_file", arguments: { path: "/tmp/测试_🎉_\n\r" } },
    });
    expect(r.status()).toBe(200);
  });

  test("deeply nested arguments (50 levels) → server doesn't crash", async ({ request }) => {
    let nested: Record<string, unknown> = {};
    let cur = nested;
    for (let i = 0; i < 50; i++) { cur.child = {}; cur = cur.child as Record<string, unknown>; }
    const r = await request.post(`${BASE}/call`, {
      data: { call_id: "deep1", name: "read_file", arguments: nested },
    });
    expect(r.status()).toBe(200);
  });

  test("extra unexpected fields → handled", async ({ request }) => {
    const r = await request.post(`${BASE}/call`, {
      data: { call_id: "extra1", name: "read_file", arguments: {}, extra: "junk", another: true },
    });
    expect(r.status()).toBe(200);
  });

  test("wrong content-type (text/plain) → 415", async ({ request }) => {
    const r = await request.post(`${BASE}/call`, {
      headers: { "Content-Type": "text/plain" },
      data: '{"call_id":"t6","name":"read_file","arguments":{}}',
    });
    expect(r.status()).toBe(415);
  });

  test("malformed JSON body → 4xx", async ({ request }) => {
    const r = await request.fetch(`${BASE}/call`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: "{broken json!!!",
    });
    expect([400, 422]).toContain(r.status());
  });

  test("empty string body → 4xx", async ({ request }) => {
    const r = await request.fetch(`${BASE}/call`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: "",
    });
    expect([400, 422]).toContain(r.status());
  });

  test("very large JSON payload (1MB) → doesn't crash", async ({ request }) => {
    const bigArgs: Record<string, string> = {};
    for (let i = 0; i < 10000; i++) { bigArgs[`key_${i}`] = "x".repeat(100); }
    const r = await request.post(`${BASE}/call`, {
      data: { call_id: "big1", name: "read_file", arguments: bigArgs },
    });
    expect([200, 400, 413]).toContain(r.status());
  });
});

// ============================================================
// 3. STATIC FILE SECURITY — PATH TRAVERSAL
// ============================================================

test.describe("Path traversal security", () => {
  test("../../../etc/passwd → 404 (no file leak)", async ({ request }) => {
    const r = await request.fetch(`${BASE}/../../../etc/passwd`);
    expect(r.status()).toBe(404);
  });

  test("URL-encoded traversal → 404 (no file leak)", async ({ request }) => {
    const r = await request.get(`${BASE}/%2e%2e/%2e%2e/%2e%2e/etc/passwd`);
    expect(r.status()).toBe(404);
  });

  test("double-encoded traversal → 404 (no file leak)", async ({ request }) => {
    const r = await request.get(`${BASE}/%252e%252e/%252e%252e/etc/passwd`);
    expect([400, 404]).toContain(r.status());
  });

  test("null byte injection → handled safely", async ({ request }) => {
    const r = await request.get(`${BASE}/assets/file%00.js`);
    expect([200, 400, 404]).toContain(r.status());
  });

  test("non-existent path → 404", async ({ request }) => {
    const r = await request.get(`${BASE}/assets/nonexistent-abc123.js`);
    expect(r.status()).toBe(404);
  });
});

// ============================================================
// 4. WEBSOCKET ERROR HANDLING
// ============================================================

test.describe("WebSocket errors", () => {
  test("GET /ws without upgrade → 400", async ({ request }) => {
    expect((await request.get(`${BASE}/ws`)).status()).toBe(400);
  });

  test("POST /ws → 405", async ({ request }) => {
    expect((await request.post(`${BASE}/ws`, { data: {} })).status()).toBe(405);
  });
});

// ============================================================
// 5. CONCURRENCY & LOAD
// ============================================================

test.describe("Concurrency stress", () => {
  test("50 concurrent /api/health → all 200", async ({ request }) => {
    const res = await Promise.all(Array(50).fill(null).map(() => request.get(`${BASE}/api/health`)));
    for (const r of res) {
      expect(r.status()).toBe(200);
      const b = await r.json();
      expect(b.status).toBe("ok");
    }
  });

  test("20 concurrent invalid /call → all return error", async ({ request }) => {
    const res = await Promise.all(Array(20).fill(null).map((_, i) =>
      request.post(`${BASE}/call`, { data: { invalid: i } })
    ));
    for (const r of res) {
      expect(r.status()).toBe(200);
      const b = await r.json();
      expect(b).toHaveProperty("error");
    }
  });

  test("mixed concurrent requests → all respond", async ({ request }) => {
    const healthReqs = Array(15).fill(null).map(() => request.get(`${BASE}/api/health`));
    const callReqs = Array(10).fill(null).map((_, i) =>
      request.post(`${BASE}/call`, { data: { call_id: `mixed_${i}`, name: "fake", arguments: {} } })
    );
    const indexReqs = Array(5).fill(null).map(() => request.get(`${BASE}/`));
    const res = await Promise.all([...healthReqs, ...callReqs, ...indexReqs]);
    for (const r of res) {
      expect([200, 404, 405]).toContain(r.status());
    }
  });
});

// ============================================================
// 6. FRONTEND RESILIENCE TO BACKEND FAILURES
// ============================================================

test.describe("Frontend resilience", () => {
  test("app renders when /api/providers returns 500", async ({ browser }) => {
    const ctx = await browser.newContext();
    const pg = await ctx.newPage();
    await pg.route("**/api/providers", (r) => r.fulfill({ status: 500, body: "fail" }));
    await pg.route("**/api/sessions", (r) => r.fulfill({ status: 200, body: "[]" }));
    await pg.goto(`${FRONTEND}/`);
    await pg.waitForTimeout(1000);
    expect(await pg.textContent("#root")).toBeTruthy();
    await ctx.close();
  });

  test("app renders when /api/sessions returns 500", async ({ browser }) => {
    const ctx = await browser.newContext();
    const pg = await ctx.newPage();
    await pg.route("**/api/providers", (r) => r.fulfill({
      status: 200, contentType: "application/json",
      body: JSON.stringify({ current: { provider: "t", model: "t" }, providers: [] }),
    }));
    await pg.route("**/api/sessions", (r) => r.fulfill({ status: 500, body: "fail" }));
    await pg.goto(`${FRONTEND}/`);
    await pg.waitForTimeout(1000);
    expect(await pg.textContent("#root")).toBeTruthy();
    await ctx.close();
  });

  test("app renders when ALL APIs return 503", async ({ browser }) => {
    const ctx = await browser.newContext();
    const pg = await ctx.newPage();
    await pg.route("**/api/**", (r) => r.fulfill({ status: 503, body: "unavailable" }));
    await pg.goto(`${FRONTEND}/`);
    await pg.waitForTimeout(1000);
    expect(await pg.textContent("#root")).toBeTruthy();
    await ctx.close();
  });

  test("app handles malformed JSON from /api/skills", async ({ browser }) => {
    const ctx = await browser.newContext();
    const pg = await ctx.newPage();
    await pg.route("**/api/providers", (r) => r.fulfill({
      status: 200, contentType: "application/json",
      body: JSON.stringify({ current: { provider: "t", model: "t" }, providers: [] }),
    }));
    await pg.route("**/api/sessions", (r) => r.fulfill({ status: 200, body: "[]" }));
    await pg.route("**/api/skills", (r) => r.fulfill({
      status: 200, contentType: "application/json", body: "not json {{{",
    }));
    await pg.goto(`${FRONTEND}/`);
    await pg.waitForTimeout(1000);
    expect(await pg.textContent("#root")).toBeTruthy();
    await ctx.close();
  });

  test("app handles massive session list (10k items)", async ({ browser }) => {
    const ctx = await browser.newContext();
    const pg = await ctx.newPage();
    const sessions = Array(10000).fill(null).map((_, i) => ({
      id: `s_${i}`, title: `Session ${i} ${"x".repeat(200)}`,
      created_at: new Date().toISOString(), updated_at: new Date().toISOString(),
    }));
    await pg.route("**/api/providers", (r) => r.fulfill({
      status: 200, contentType: "application/json",
      body: JSON.stringify({ current: { provider: "t", model: "t" }, providers: [] }),
    }));
    await pg.route("**/api/sessions", (r) => r.fulfill({
      status: 200, contentType: "application/json", body: JSON.stringify(sessions),
    }));
    await pg.goto(`${FRONTEND}/`);
    await pg.waitForTimeout(3000);
    expect(await pg.textContent("#root")).toBeTruthy();
    await ctx.close();
  });
});

// ============================================================
// 7. TIMEOUT & SLOW RESPONSE HANDLING
// ============================================================

test.describe("Slow/failed responses", () => {
  test("app renders during 5s API delay", async ({ browser }) => {
    const ctx = await browser.newContext();
    const pg = await ctx.newPage();
    await pg.route("**/api/providers", async (r) => {
      await new Promise((res) => setTimeout(res, 5000));
      await r.fulfill({
        status: 200, contentType: "application/json",
        body: JSON.stringify({ current: { provider: "t", model: "t" }, providers: [] }),
      });
    });
    await pg.route("**/api/sessions", (r) => r.fulfill({ status: 200, body: "[]" }));
    await pg.goto(`${FRONTEND}/`);
    await pg.waitForTimeout(1000);
    expect(await pg.textContent("#root")).toBeTruthy();
    await ctx.close();
  });

  test("app handles aborted API request (timeout)", async ({ browser }) => {
    const ctx = await browser.newContext();
    const pg = await ctx.newPage();
    await pg.route("**/api/skills", (r) => r.abort("timedout"));
    await pg.route("**/api/providers", (r) => r.fulfill({
      status: 200, contentType: "application/json",
      body: JSON.stringify({ current: { provider: "t", model: "t" }, providers: [] }),
    }));
    await pg.route("**/api/sessions", (r) => r.fulfill({ status: 200, body: "[]" }));
    await pg.goto(`${FRONTEND}/`);
    await pg.waitForTimeout(2000);
    expect(await pg.textContent("#root")).toBeTruthy();
    await ctx.close();
  });
});

// ============================================================
// 8. RESPONSE STRUCTURE VALIDATION
// ============================================================

test.describe("API response structure", () => {
  test("/api/health returns valid JSON", async ({ request }) => {
    const r = await request.get(`${BASE}/api/health`);
    expect(r.status()).toBe(200);
    const b = await r.json();
    expect(b).toHaveProperty("status");
    expect(b.status).toBe("ok");
  });

  test("/ returns HTML", async ({ request }) => {
    const r = await request.get(`${BASE}/`);
    expect(r.status()).toBe(200);
    const t = await r.text();
    expect(t).toContain("<!DOCTYPE html>");
    expect(t).toContain("RustyCode");
  });

  test("POST /call with valid structure returns call_id", async ({ request }) => {
    const r = await request.post(`${BASE}/call`, {
      data: { call_id: "struct_test", name: "fake_tool", arguments: {} },
    });
    expect(r.status()).toBe(200);
    const b = await r.json();
    expect(b.call_id).toBe("struct_test");
    expect(b).toHaveProperty("success");
    expect(b.success).toBe(false);
    expect(b).toHaveProperty("error");
  });

  test("POST /call error for missing field includes field name", async ({ request }) => {
    const r = await request.post(`${BASE}/call`, { data: {} });
    expect(r.status()).toBe(200);
    const b = await r.json();
    expect(b.error).toMatch(/call_id|name|arguments/);
  });

  test("non-existent API route → 404", async ({ request }) => {
    expect((await request.get(`${BASE}/api/nonexistent`)).status()).toBe(404);
  });

  test("/api/skills → 404 (not served by backend)", async ({ request }) => {
    expect((await request.get(`${BASE}/api/skills`)).status()).toBe(404);
  });

  test("/api/sessions → 404 (not served by backend)", async ({ request }) => {
    expect((await request.get(`${BASE}/api/sessions`)).status()).toBe(404);
  });

  test("/api/mcp/servers → 404 (not served by backend)", async ({ request }) => {
    expect((await request.get(`${BASE}/api/mcp/servers`)).status()).toBe(404);
  });
});
