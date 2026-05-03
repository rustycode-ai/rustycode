import { test as base, type Page } from "@playwright/test";

type Fixtures = {
  appPage: Page;
};

export const test = base.extend<Fixtures>({
  appPage: async ({ page }, use) => {
    await page.route("**/api/providers", (route) =>
      route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({
          current: { provider: "mock", model: "mock-model" },
        }),
      }),
    );

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

export const META = process.platform === "darwin" ? "Meta" : "Control";
export { expect } from "@playwright/test";
