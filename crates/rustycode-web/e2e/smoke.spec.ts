import { test, expect } from "./fixtures";

test.describe("smoke", () => {
  test("page loads with correct title", async ({ appPage }) => {
    await expect(appPage).toHaveTitle("RustyCode");
  });

  test("root element renders", async ({ appPage }) => {
    await expect(appPage.locator("#root")).toBeVisible();
  });
});
