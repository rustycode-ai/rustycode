import { test, expect, META } from "./fixtures";

test.describe("accessibility", () => {
  test("message list has role=log and aria-label", async ({ appPage }) => {
    const list = appPage.locator(".message-list");
    await expect(list).toBeVisible();
    await expect(list).toHaveAttribute("role", "log");
    await expect(list).toHaveAttribute(
      "aria-label",
      "Conversation messages",
    );
  });

  test("command palette has dialog role and aria-modal", async ({
    appPage,
  }) => {
    await appPage.locator("textarea").blur();
    await appPage.keyboard.press(`${META}+k`);
    const overlay = appPage.locator(".palette-overlay");
    await expect(overlay).toHaveAttribute("role", "dialog");
    await expect(overlay).toHaveAttribute("aria-modal", "true");
    await expect(overlay).toHaveAttribute("aria-label", "Command palette");
  });

  test("search overlay has dialog role and aria-label", async ({
    appPage,
  }) => {
    await appPage.locator("textarea").blur();
    await appPage.keyboard.press(`${META}+f`);
    const overlay = appPage.locator(".search-overlay");
    await expect(overlay).toHaveAttribute("role", "dialog");
    await expect(overlay).toHaveAttribute("aria-label", "Search messages");
  });

  test("shortcuts overlay has dialog role and aria-label", async ({
    appPage,
  }) => {
    await appPage.locator("textarea").blur();
    await appPage.keyboard.press(`${META}+/`);
    const overlay = appPage.locator(".shortcuts-overlay");
    await expect(overlay).toHaveAttribute("role", "dialog");
    await expect(overlay).toHaveAttribute(
      "aria-label",
      "Keyboard shortcuts",
    );
  });

  test("model selector modal has dialog role and aria-modal", async ({
    appPage,
  }) => {
    await appPage.locator("button[aria-label='Switch model']").click();
    const overlay = appPage.locator(".model-modal-overlay");
    await expect(overlay).toHaveAttribute("role", "dialog");
    await expect(overlay).toHaveAttribute("aria-modal", "true");
  });

  test("skip link navigates to main content", async ({ appPage }) => {
    const skipLink = appPage.locator(".skip-link");
    await expect(skipLink).toHaveAttribute("href", "#main-content");
    await expect(skipLink).toHaveText("Skip to content");

    const main = appPage.locator("#main-content");
    await expect(main).toBeVisible();
  });

  test("connection status dot has accessible label", async ({ appPage }) => {
    const dot = appPage.locator(".status-dot");
    await expect(dot).toHaveAttribute("title", /connecting|connected|disconnected/i);
  });
});
