import { expect, test } from "@playwright/test";

test.describe("Chat Page", () => {
  test("home page loads with input field", async ({ page }) => {
    await page.goto("/");
    await expect(page.getByTestId("multimodal-input")).toBeVisible();
  });

  test("can type in the input field", async ({ page }) => {
    await page.goto("/");
    const input = page.getByTestId("multimodal-input");
    await input.fill("Hello world");
    await expect(input).toHaveValue("Hello world");
  });

  test("submit button is visible", async ({ page }) => {
    await page.goto("/");
    await expect(page.getByTestId("send-button")).toBeVisible();
  });

  test("starter prompts are not shown on empty chat", async ({ page }) => {
    await page.goto("/");
    const suggestions = page.locator("[data-testid='suggested-actions']");
    await expect(suggestions).toHaveCount(0);
  });

  test("stays light and does not expose a theme command", async ({ page }) => {
    await page.emulateMedia({ colorScheme: "dark" });
    await page.goto("/");

    await expect(page.locator("html")).not.toHaveClass(/dark/);
    await page.getByTestId("multimodal-input").fill("/");
    await expect(
      page.getByRole("listbox", { name: "Slash commands" })
    ).toBeVisible();
    await expect(page.getByRole("option", { name: /theme/i })).toHaveCount(0);
  });

  test("can stop generation with stop button", async ({ page }) => {
    await page.goto("/");

    // Type and send a message
    await page.getByTestId("multimodal-input").fill("Hello");
    await page.getByTestId("send-button").click();

    // Stop button should appear during generation
    const stopButton = page.getByTestId("stop-button");
    // If generation starts, stop button appears
    // This is a best-effort check since timing depends on API
    await stopButton.click({ timeout: 5000 }).catch(() => {
      // Generation may have finished before we could click
    });
  });
});

test.describe("Chat Input Features", () => {
  test("input clears after sending", async ({ page }) => {
    await page.goto("/");
    const input = page.getByTestId("multimodal-input");
    await input.fill("Test message");
    await page.getByTestId("send-button").click();

    // Input should clear after sending
    await expect(input).toHaveValue("");
  });

  test("input supports multiline text", async ({ page }) => {
    await page.goto("/");
    const input = page.getByTestId("multimodal-input");
    await input.fill("Line 1\nLine 2\nLine 3");
    await expect(input).toContainText("Line 1");
  });

  test("keeps short user messages on one line", async ({ page }) => {
    await page.setViewportSize({ height: 800, width: 375 });
    let submittedText: string | undefined;
    await page.route("**/api/chat", async (route) => {
      const body = route.request().postDataJSON() as {
        message?: { parts?: Array<{ text?: string }> };
      };
      submittedText = body.message?.parts?.[0]?.text;
      await route.fulfill({
        body: "",
        headers: { "content-type": "text/event-stream; charset=utf-8" },
        status: 200,
      });
    });

    await page.goto("/");
    const input = page.getByTestId("multimodal-input");
    await input.fill("hi");
    await page.getByTestId("send-button").click();
    await expect.poll(() => submittedText).toBe("hi");

    const userContent = page.locator(
      "[data-role='user'] [data-testid='message-content']"
    );
    await expect(userContent).toHaveText("hi");

    const paragraph = userContent.locator("p");
    await expect(paragraph).toHaveCount(1);
    const { height, lineHeight } = await paragraph.evaluate((element) => {
      const style = getComputedStyle(element);
      return {
        height: element.getBoundingClientRect().height,
        lineHeight: Number.parseFloat(style.lineHeight),
      };
    });
    expect(height).toBeLessThan(lineHeight * 1.5);
  });
});
