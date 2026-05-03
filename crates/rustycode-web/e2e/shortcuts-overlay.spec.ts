import { test, expect, META } from "./fixtures";

test.describe("shortcuts overlay", () => {
  test.beforeEach(async ({ appPage }) => {
    await appPage.locator("textarea").blur();
  });

  test("shows Keyboard Shortcuts heading", async ({ appPage }) => {
    await appPage.keyboard.press(`${META}+/`);
    await expect(appPage.locator(".shortcuts-overlay")).toBeVisible();
    await expect(
      appPage.locator(".shortcuts-header h2"),
    ).toHaveText("Keyboard Shortcuts");
  });

  test("contains expected shortcut labels", async ({ appPage }) => {
    await appPage.keyboard.press(`${META}+/`);
    const labels = appPage.locator(".shortcut-label");
    await expect(labels).toHaveCount(10);

    const labelTexts = await labels.allTextContents();
    expect(labelTexts).toContain("Command palette");
    expect(labelTexts).toContain("Toggle sidebar");
    expect(labelTexts).toContain("Search messages");
    expect(labelTexts).toContain("Keyboard shortcuts");
    expect(labelTexts).toContain("Send message");
    expect(labelTexts).toContain("Close dialog / Stop generation");
  });

  test("Escape closes the overlay", async ({ appPage }) => {
    await appPage.keyboard.press(`${META}+/`);
    await expect(appPage.locator(".shortcuts-overlay")).toBeVisible();
    await appPage.keyboard.press("Escape");
    await expect(appPage.locator(".shortcuts-overlay")).not.toBeVisible();
  });

  test("close button closes the overlay", async ({ appPage }) => {
    await appPage.keyboard.press(`${META}+/`);
    await expect(appPage.locator(".shortcuts-overlay")).toBeVisible();
    await appPage
      .locator(".shortcuts-panel button[aria-label='Close']")
      .click();
    await expect(appPage.locator(".shortcuts-overlay")).not.toBeVisible();
  });

  test("clicking backdrop closes the overlay", async ({ appPage }) => {
    await appPage.keyboard.press(`${META}+/`);
    await expect(appPage.locator(".shortcuts-overlay")).toBeVisible();
    await appPage.locator(".shortcuts-overlay").click({ position: { x: 5, y: 5 } });
    await expect(appPage.locator(".shortcuts-overlay")).not.toBeVisible();
  });

  test("has role=dialog and aria-label", async ({ appPage }) => {
    await appPage.keyboard.press(`${META}+/`);
    const overlay = appPage.locator(".shortcuts-overlay");
    await expect(overlay).toHaveAttribute("role", "dialog");
    await expect(overlay).toHaveAttribute(
      "aria-label",
      "Keyboard shortcuts",
    );
  });
});
