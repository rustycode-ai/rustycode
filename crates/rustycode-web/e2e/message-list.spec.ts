import { test, expect } from "./fixtures";

test.describe("message list", () => {
  test("empty state shows heading and hint", async ({ appPage }) => {
    await expect(appPage.locator(".message-empty h2")).toHaveText("RustyCode");
    await expect(appPage.locator(".message-empty p")).toHaveText(
      "Send a message to start a conversation.",
    );
  });

  test("empty state shows keyboard hints", async ({ appPage }) => {
    const hints = appPage.locator(".empty-hint");
    await expect(hints).toHaveCount(3);
    await expect(hints.nth(0)).toContainText("Commands");
    await expect(hints.nth(1)).toContainText("Sessions");
    await expect(hints.nth(2)).toContainText("Shortcuts");
  });

  test("sending a message renders user bubble with correct content", async ({
    appPage,
  }) => {
    const textarea = appPage.locator("textarea[aria-label='Message input']");
    await textarea.fill("Hello from E2E");
    await textarea.press("Enter");

    const bubble = appPage.locator(
      ".message-bubble.message-user [data-message-id], .message-user[data-message-id]",
    );
    await expect(bubble.first()).toBeVisible();
    await expect(
      appPage.locator(".message-user .message-content"),
    ).toContainText("Hello from E2E");
  });

  test("message bubble has data-message-id attribute", async ({ appPage }) => {
    const textarea = appPage.locator("textarea[aria-label='Message input']");
    await textarea.fill("ID check");
    await textarea.press("Enter");

    const msg = appPage.locator("[data-message-id]").first();
    await expect(msg).toBeVisible();
    const id = await msg.getAttribute("data-message-id");
    expect(id).toBeTruthy();
  });

  test("message list has role=log for accessibility", async ({ appPage }) => {
    await expect(
      appPage.locator(".message-list[role='log']"),
    ).toBeVisible();
  });
});
