import { test, expect, META } from "./fixtures";

test.describe("keyboard shortcuts", () => {
  // Blur the textarea so it doesn't capture keyboard events
  test.beforeEach(async ({ appPage }) => {
    await appPage.locator("textarea").blur();
  });

  test("Meta+K opens command palette", async ({ appPage }) => {
    await appPage.keyboard.press(`${META}+k`);
    await expect(appPage.locator(".palette-overlay")).toBeVisible();
  });

  test("Meta+B toggles sidebar", async ({ appPage }) => {
    // Sidebar is open by default — close it
    await appPage.keyboard.press(`${META}+b`);
    await expect(appPage.locator("aside.session-sidebar")).not.toBeVisible();

    // Reopen
    await appPage.keyboard.press(`${META}+b`);
    await expect(appPage.locator("aside.session-sidebar")).toBeVisible();
  });

  test("Meta+/ opens shortcuts overlay", async ({ appPage }) => {
    await appPage.keyboard.press(`${META}+/`);
    await expect(appPage.locator(".shortcuts-overlay")).toBeVisible();
  });

  test("Meta+F opens search overlay", async ({ appPage }) => {
    await appPage.keyboard.press(`${META}+f`);
    await expect(appPage.locator(".search-overlay")).toBeVisible();
  });
});
