import { test, expect } from "./fixtures";

test.describe("chat input", () => {
  test("user can type into the input textarea", async ({ appPage }) => {
    const textarea = appPage.locator("textarea[aria-label='Message input']");
    await textarea.fill("Hello, RustyCode!");
    await expect(textarea).toHaveValue("Hello, RustyCode!");
  });

  test("pressing Enter submits the message", async ({ appPage }) => {
    const textarea = appPage.locator("textarea[aria-label='Message input']");
    await textarea.fill("Test message");
    await textarea.press("Enter");

    // Message should appear in the message list
    const message = appPage.locator("[data-message-id]");
    await expect(message.first()).toBeVisible();
  });

  test("textarea is cleared after sending", async ({ appPage }) => {
    const textarea = appPage.locator("textarea[aria-label='Message input']");
    await textarea.fill("Clear test");
    await textarea.press("Enter");

    await expect(textarea).toHaveValue("");
  });

  test("send button submits message", async ({ appPage }) => {
    const textarea = appPage.locator("textarea[aria-label='Message input']");
    await textarea.fill("Button send test");

    const sendBtn = appPage.locator("button[aria-label='Send message']");
    await sendBtn.click();

    const message = appPage.locator("[data-message-id]");
    await expect(message.first()).toBeVisible();
  });
});
