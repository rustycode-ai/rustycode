import { test, expect, META } from "./fixtures";

test.describe("command palette", () => {
  test.beforeEach(async ({ appPage }) => {
    await appPage.locator("textarea").blur();
  });

  test("opens with search input focused", async ({ appPage }) => {
    await appPage.keyboard.press(`${META}+k`);
    await expect(appPage.locator(".palette-overlay")).toBeVisible();
    const input = appPage.locator(".palette-search");
    await expect(input).toBeVisible();
    await expect(input).toBeFocused();
  });

  test("shows actions section with expected items", async ({ appPage }) => {
    await appPage.keyboard.press(`${META}+k`);
    await expect(appPage.locator(".palette-section-header").first()).toHaveText(
      "Actions",
    );
    await expect(
      appPage.locator(".palette-item-label").getByText("Toggle Sidebar"),
    ).toBeVisible();
  });

  test("typing filters items", async ({ appPage }) => {
    await appPage.keyboard.press(`${META}+k`);
    const input = appPage.locator(".palette-search");
    await input.fill("export");

    const items = appPage.locator(".palette-item");
    await expect(items).toHaveCount(1);
    await expect(items.first()).toContainText("Export Conversation");
  });

  test("ArrowDown moves highlight to next item", async ({ appPage }) => {
    await appPage.keyboard.press(`${META}+k`);
    const first = appPage.locator("[data-highlighted='true']").first();
    await expect(first).toBeVisible();

    await appPage.keyboard.press("ArrowDown");
    const highlighted = appPage.locator(".palette-item-highlight");
    await expect(highlighted).toHaveCount(1);
  });

  test("ArrowUp moves highlight back", async ({ appPage }) => {
    await appPage.keyboard.press(`${META}+k`);
    await appPage.keyboard.press("ArrowDown");
    await appPage.keyboard.press("ArrowUp");

    const highlighted = appPage.locator(".palette-item-highlight");
    await expect(highlighted).toHaveCount(1);
  });

  test("Escape closes the palette", async ({ appPage }) => {
    await appPage.keyboard.press(`${META}+k`);
    await expect(appPage.locator(".palette-overlay")).toBeVisible();
    await appPage.keyboard.press("Escape");
    await expect(appPage.locator(".palette-overlay")).not.toBeVisible();
  });

  test("no matching search shows empty state", async ({ appPage }) => {
    await appPage.keyboard.press(`${META}+k`);
    await appPage.locator(".palette-search").fill("zzzznonexistent");
    await expect(appPage.locator(".palette-empty")).toHaveText(
      "No results found.",
    );
  });

  test("has role=dialog and aria-modal", async ({ appPage }) => {
    await appPage.keyboard.press(`${META}+k`);
    const overlay = appPage.locator(".palette-overlay");
    await expect(overlay).toHaveAttribute("role", "dialog");
    await expect(overlay).toHaveAttribute("aria-modal", "true");
  });
});
