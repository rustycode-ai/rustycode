import { test, expect } from "./fixtures";

test.describe("model selector", () => {
  test("opens when clicking model selector button", async ({ appPage }) => {
    await appPage.locator("button[aria-label='Switch model']").click();
    await expect(appPage.locator(".model-modal-overlay")).toBeVisible();
  });

  test("shows search input with correct placeholder", async ({ appPage }) => {
    await appPage.locator("button[aria-label='Switch model']").click();
    const input = appPage.locator(".model-search");
    await expect(input).toBeVisible();
    await expect(input).toHaveAttribute("placeholder", "Search models...");
  });

  test("lists provider groups with model items", async ({ appPage }) => {
    await appPage.locator("button[aria-label='Switch model']").click();
    await expect(appPage.locator(".model-group").first()).toBeVisible();
    await expect(appPage.locator(".model-item").first()).toBeVisible();
  });

  test("search filters models", async ({ appPage }) => {
    await appPage.locator("button[aria-label='Switch model']").click();
    const input = appPage.locator(".model-search");
    await input.fill("zzzznonexistent");
    await expect(appPage.locator(".model-empty")).toHaveText(
      "No models match your search.",
    );
  });

  test("Escape closes model selector", async ({ appPage }) => {
    await appPage.locator("button[aria-label='Switch model']").click();
    await expect(appPage.locator(".model-modal-overlay")).toBeVisible();
    await appPage.keyboard.press("Escape");
    await expect(appPage.locator(".model-modal-overlay")).not.toBeVisible();
  });

  test("clicking backdrop closes model selector", async ({ appPage }) => {
    await appPage.locator("button[aria-label='Switch model']").click();
    await expect(appPage.locator(".model-modal-overlay")).toBeVisible();
    await appPage.locator(".model-modal-overlay").click({ position: { x: 5, y: 5 } });
    await expect(appPage.locator(".model-modal-overlay")).not.toBeVisible();
  });

  test("has role=dialog and aria-modal", async ({ appPage }) => {
    await appPage.locator("button[aria-label='Switch model']").click();
    const overlay = appPage.locator(".model-modal-overlay");
    await expect(overlay).toHaveAttribute("role", "dialog");
    await expect(overlay).toHaveAttribute("aria-modal", "true");
  });
});
