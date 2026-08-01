import { expect, test } from "@playwright/test";

test.describe("Mobile layout", () => {
  test.use({
    isMobile: true,
    viewport: { height: 844, width: 390 },
  });

  test("opens and closes the chat sidebar", async ({ page }) => {
    await page.goto("/");

    const sidebar = page.locator("[data-sidebar=sidebar]");
    await expect(page.getByTestId("multimodal-input")).toBeVisible();
    await expect(sidebar).not.toBeVisible();

    await page.getByLabel("Toggle Sidebar").first().click();
    await expect(sidebar).toBeVisible();

    await page.locator("[data-slot='sheet-overlay']").click({
      position: { x: 8, y: 8 },
    });
    await expect(sidebar).not.toBeVisible();
  });
});
