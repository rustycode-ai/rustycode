import { test as base, type Page } from "@playwright/test";

/**
 * Extend Playwright test with rustycode-web fixtures.
 * Mocks /api/providers so the app renders without a backend.
 */
type Fixtures = {
  appPage: Page;
};

export const test = base.extend<Fixtures>({
  appPage: async ({ page }, use) => {
    // Mock the providers API so StatusBar doesn't fail
    await page.route("**/api/providers", (route) =>
      route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({
          current: { provider: "mock", model: "mock-model" },
        }),
      }),
    );

    // Mock session list for sidebar
    await page.route("**/api/sessions", (route) =>
      route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify([]),
      }),
    );

    await page.goto("/");
    await use(page);
  },
});

export { expect } from "@playwright/test";
