import { test, expect } from "@playwright/test";

test("homepage has correct title", async ({ page }) => {
  await page.goto("/");
  await expect(page).toHaveTitle(/{{PROJECT_NAME}}/);
});

test("homepage shows project name", async ({ page }) => {
  await page.goto("/");
  const heading = page.getByRole("heading", { level: 1 });
  await expect(heading).toBeVisible();
});
