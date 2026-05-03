import { test, expect, META } from "./fixtures";

test.describe("session sidebar", () => {
  test.beforeEach(async ({ appPage }) => {
    await appPage.locator("textarea").blur();
  });

  test("sidebar is visible on initial load", async ({ appPage }) => {
    await expect(appPage.locator("aside.session-sidebar")).toBeVisible();
  });

  test("clicking close button closes sidebar", async ({ appPage }) => {
    await expect(appPage.locator("aside.session-sidebar")).toBeVisible();

    await appPage.locator("aside.session-sidebar button[aria-label='Close sidebar']").click();
    await expect(appPage.locator("aside.session-sidebar")).not.toBeVisible();
  });

  test("sidebar contains new session button", async ({ appPage }) => {
    await expect(
      appPage.locator("aside.session-sidebar button[aria-label='New session']"),
    ).toBeVisible();
  });

  test("Meta+B reopens sidebar after closing", async ({ appPage }) => {
    // Close via close button
    await appPage.locator("aside.session-sidebar button[aria-label='Close sidebar']").click();
    await expect(appPage.locator("aside.session-sidebar")).not.toBeVisible();

    // Reopen via keyboard
    await appPage.keyboard.press(`${META}+b`);
    await expect(appPage.locator("aside.session-sidebar")).toBeVisible();
  });
});
