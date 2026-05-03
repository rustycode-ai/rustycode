import { test, expect } from "./fixtures";

test.describe("app shell", () => {
  test("status bar is visible", async ({ appPage }) => {
    await expect(appPage.locator("header.status-bar")).toBeVisible();
  });

  test("connection status dot exists", async ({ appPage }) => {
    await expect(appPage.locator(".status-dot")).toBeVisible();
  });

  test("input bar is visible with textarea", async ({ appPage }) => {
    await expect(appPage.locator("textarea[aria-label='Message input']")).toBeVisible();
  });

  test("send button exists", async ({ appPage }) => {
    await expect(appPage.locator("button[aria-label='Send message']")).toBeVisible();
  });

  test("main content area exists", async ({ appPage }) => {
    await expect(appPage.locator("#main-content")).toBeVisible();
  });

  test("skip-to-content link for accessibility", async ({ appPage }) => {
    await expect(appPage.locator("a.skip-link")).toBeAttached();
  });
});
