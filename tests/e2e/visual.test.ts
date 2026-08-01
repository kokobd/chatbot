import { expect, type Page, test } from "@playwright/test";

async function preparePage(page: Page) {
  await page.goto("/");
  await page.evaluate(() => document.documentElement.classList.remove("dark"));
  await page.context().addCookies([
    {
      name: "sidebar_state",
      url: new URL(page.url()).origin,
      value: "true",
    },
  ]);
  await page.reload();
  await expect(page.getByTestId("multimodal-input")).toBeVisible();
}

test.describe("Light UI visual baseline", () => {
  test("empty chat desktop", async ({ page }) => {
    await preparePage(page);

    await expect(page).toHaveScreenshot("empty-chat-desktop.png", {
      animations: "disabled",
      fullPage: true,
      mask: [page.getByTestId("user-email")],
      maxDiffPixelRatio: 0.01,
    });
  });

  test.describe("mobile", () => {
    test.use({
      isMobile: true,
      viewport: { height: 844, width: 390 },
    });

    test("empty chat mobile", async ({ page }) => {
      await preparePage(page);

      await expect(page).toHaveScreenshot("empty-chat-mobile.png", {
        animations: "disabled",
        fullPage: true,
        maxDiffPixelRatio: 0.01,
      });
    });
  });
});
