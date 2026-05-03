import { test, expect, META } from "./fixtures";

test.describe("search overlay", () => {
  test.beforeEach(async ({ appPage }) => {
    await appPage.locator("textarea").blur();
  });

  test("search input is auto-focused when overlay opens", async ({
    appPage,
  }) => {
    await appPage.keyboard.press(`${META}+f`);
    await expect(appPage.locator(".search-overlay")).toBeVisible();
    await expect(appPage.locator(".search-input")).toBeFocused();
  });

  test("typing matching query shows results", async ({ appPage }) => {
    // First send a message to have something to search
    const textarea = appPage.locator("textarea[aria-label='Message input']");
    await textarea.fill("Unique searchable content about Rust");
    await textarea.press("Enter");
    await expect(appPage.locator("[data-message-id]").first()).toBeVisible();

    // Now search
    await appPage.keyboard.press(`${META}+f`);
    const input = appPage.locator(".search-input");
    await input.fill("Rust");

    const results = appPage.locator(".search-result");
    await expect(results).toHaveCount(1);
    await expect(results.first()).toContainText("Rust");
  });

  test("no matches shows indicator", async ({ appPage }) => {
    await appPage.keyboard.press(`${META}+f`);
    const input = appPage.locator(".search-input");
    await input.fill("zzzznothingmatchesthis");

    await expect(appPage.locator(".search-count")).toContainText("No results");
  });

  test("result count updates", async ({ appPage }) => {
    // Send a message
    const textarea = appPage.locator("textarea[aria-label='Message input']");
    await textarea.fill("Find me in search");
    await textarea.press("Enter");
    await expect(appPage.locator("[data-message-id]").first()).toBeVisible();

    // Search for it
    await appPage.keyboard.press(`${META}+f`);
    await appPage.locator(".search-input").fill("Find me");
    await expect(appPage.locator(".search-count")).toContainText("1/1");
  });

  test("Escape closes the search overlay", async ({ appPage }) => {
    await appPage.keyboard.press(`${META}+f`);
    await expect(appPage.locator(".search-overlay")).toBeVisible();
    await appPage.keyboard.press("Escape");
    await expect(appPage.locator(".search-overlay")).not.toBeVisible();
  });

  test("close button closes the overlay", async ({ appPage }) => {
    await appPage.keyboard.press(`${META}+f`);
    await expect(appPage.locator(".search-overlay")).toBeVisible();
    await appPage.locator("button[aria-label='Close search']").click();
    await expect(appPage.locator(".search-overlay")).not.toBeVisible();
  });

  test("keyboard navigation with ArrowDown and ArrowUp", async ({
    appPage,
  }) => {
    // Send two messages with distinct content
    const textarea = appPage.locator("textarea[aria-label='Message input']");
    await textarea.fill("alpha search term");
    await textarea.press("Enter");
    await expect(appPage.locator("[data-message-id]").first()).toBeVisible();
    await textarea.fill("beta search term");
    await textarea.press("Enter");

    // Search
    await appPage.keyboard.press(`${META}+f`);
    const input = appPage.locator(".search-input");
    await input.fill("search term");

    const results = appPage.locator(".search-result");
    await expect(results).toHaveCount(2);

    // ArrowDown should move selection
    await input.press("ArrowDown");
    const selected = appPage.locator(".search-result.selected");
    await expect(selected).toHaveCount(1);

    // ArrowUp should move back
    await input.press("ArrowUp");
    await expect(selected).toHaveCount(1);
  });

  test("has role=dialog and aria-label", async ({ appPage }) => {
    await appPage.keyboard.press(`${META}+f`);
    const overlay = appPage.locator(".search-overlay");
    await expect(overlay).toHaveAttribute("role", "dialog");
    await expect(overlay).toHaveAttribute("aria-label", "Search messages");
  });
});
