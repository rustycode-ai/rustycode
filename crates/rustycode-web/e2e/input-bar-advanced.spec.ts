import { test, expect } from "./fixtures";

test.describe("input bar advanced features", () => {
  test("character count appears after typing", async ({ appPage }) => {
    const textarea = appPage.locator("textarea[aria-label='Message input']");
    await expect(appPage.locator(".input-char-count")).not.toBeVisible();

    await textarea.fill("Hello");
    const charCount = appPage.locator(".input-char-count");
    await expect(charCount).toBeVisible();
    await expect(charCount).toHaveAttribute("aria-label", "5 characters");
  });

  test("character count hides when textarea is cleared", async ({
    appPage,
  }) => {
    const textarea = appPage.locator("textarea[aria-label='Message input']");
    await textarea.fill("Hello");
    await expect(appPage.locator(".input-char-count")).toBeVisible();

    await textarea.fill("");
    await expect(appPage.locator(".input-char-count")).not.toBeVisible();
  });

  test("stop button appears after sending a message", async ({ appPage }) => {
    const textarea = appPage.locator("textarea[aria-label='Message input']");
    await textarea.fill("Test pending");
    await textarea.press("Enter");

    const stopBtn = appPage.locator("button[aria-label='Stop generation']");
    await expect(stopBtn).toBeVisible();
  });

  test("regenerate button is visible when not pending", async ({
    appPage,
  }) => {
    await expect(
      appPage.locator("button[aria-label='Regenerate last response']"),
    ).toBeVisible();
  });

  test("regenerate button hidden while pending", async ({ appPage }) => {
    const textarea = appPage.locator("textarea[aria-label='Message input']");
    await textarea.fill("Test regenerate");
    await textarea.press("Enter");

    await expect(
      appPage.locator("button[aria-label='Regenerate last response']"),
    ).not.toBeVisible();
  });

  test("placeholder changes when pending", async ({ appPage }) => {
    const textarea = appPage.locator("textarea[aria-label='Message input']");
    await expect(textarea).toHaveAttribute(
      "placeholder",
      /Message RustyCode/,
    );

    await textarea.fill("Test placeholder");
    await textarea.press("Enter");

    await expect(textarea).toHaveAttribute(
      "placeholder",
      /Response in progress/,
    );
  });
});
