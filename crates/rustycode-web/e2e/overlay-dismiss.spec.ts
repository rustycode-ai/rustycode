import { test, expect, META } from "./fixtures";

test.describe("overlay backdrop dismiss", () => {
  test.beforeEach(async ({ appPage }) => {
    await appPage.locator("textarea").blur();
  });

  test("clicking command palette backdrop closes it", async ({ appPage }) => {
    await appPage.keyboard.press(`${META}+k`);
    await expect(appPage.locator(".palette-overlay")).toBeVisible();
    await appPage.locator(".palette-overlay").click({ position: { x: 5, y: 5 } });
    await expect(appPage.locator(".palette-overlay")).not.toBeVisible();
  });

  test("clicking search overlay backdrop closes it", async ({ appPage }) => {
    await appPage.keyboard.press(`${META}+f`);
    await expect(appPage.locator(".search-overlay")).toBeVisible();
    await appPage.locator(".search-overlay").click({ position: { x: 5, y: 5 } });
    await expect(appPage.locator(".search-overlay")).not.toBeVisible();
  });

  test("clicking shortcuts overlay backdrop closes it", async ({
    appPage,
  }) => {
    await appPage.keyboard.press(`${META}+/`);
    await expect(appPage.locator(".shortcuts-overlay")).toBeVisible();
    await appPage.locator(".shortcuts-overlay").click({ position: { x: 5, y: 5 } });
    await expect(appPage.locator(".shortcuts-overlay")).not.toBeVisible();
  });

  test("clicking model selector backdrop closes it", async ({ appPage }) => {
    await appPage.locator("button[aria-label='Switch model']").click();
    await expect(appPage.locator(".model-modal-overlay")).toBeVisible();
    await appPage.locator(".model-modal-overlay").click({ position: { x: 5, y: 5 } });
    await expect(appPage.locator(".model-modal-overlay")).not.toBeVisible();
  });
});
