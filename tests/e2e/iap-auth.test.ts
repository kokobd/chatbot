import { expect, test } from "@playwright/test";

test.describe("IAP authentication", () => {
  test("uses the injected IAP identity", async ({ page }) => {
    await page.goto("/");
    await expect(page.getByTestId("user-nav-button")).toBeVisible();
    const configuredEmail =
      process.env.IAP_TEST_EMAIL ?? "playwright@example.com";
    const [localPart, domain] = configuredEmail.split("@");
    await expect(page.getByTestId("user-email")).toContainText(localPart);
    await expect(page.getByTestId("user-email")).toContainText(domain);
  });

  test("sign out uses the IAP sign-out flow", async ({ page }) => {
    await page.goto("/");
    await page.getByTestId("user-nav-button").click();
    await page.getByTestId("user-nav-item-auth").click();
    await expect(page).toHaveURL(/gcp-iap-mode=GCIP_SIGNOUT/);
  });
});
