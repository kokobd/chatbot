import { expect, test } from "@playwright/test";
import { UI_MESSAGE_STREAM_HEADERS } from "ai";

const categorizedError =
  "OpenRouter does not have enough credits for this request. Increase the key limit or use a smaller output limit, then try again.";

test("shows the categorized streamed chat error in the existing toast", async ({
  page,
}) => {
  await page.route("**/api/chat", async (route) => {
    await route.fulfill({
      body: `data: ${JSON.stringify({ errorText: categorizedError, type: "error" })}\n\n`,
      headers: UI_MESSAGE_STREAM_HEADERS,
      status: 200,
    });
  });

  await page.goto("/");
  await page.getByTestId("multimodal-input").fill("Test provider error");
  await page.getByTestId("send-button").click();

  await expect(page.getByText(categorizedError, { exact: true })).toBeVisible({
    timeout: 5000,
  });
});
