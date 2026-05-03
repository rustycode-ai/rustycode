import { test, expect } from "./fixtures";

test.describe("settings panel", () => {
  test("clicking settings button opens panel", async ({ appPage }) => {
    await appPage.locator("button[aria-label='Settings']").click();
    await expect(appPage.locator(".settings-panel")).toBeVisible();
  });

  test("settings panel displays provider and model", async ({ appPage }) => {
    await appPage.locator("button[aria-label='Settings']").click();
    const panel = appPage.locator(".settings-panel");
    await expect(panel).toContainText("mock");
  });

  test("closing settings panel hides it", async ({ appPage }) => {
    await appPage.locator("button[aria-label='Settings']").click();
    await expect(appPage.locator(".settings-panel")).toBeVisible();

    // Close via the close button inside the panel
    await appPage.locator(".settings-panel button[aria-label='Close settings']").click();
    await expect(appPage.locator(".settings-panel")).not.toBeVisible();
  });
});
