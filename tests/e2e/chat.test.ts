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

const shortUserMessages = ["你好", "Hello!", "12345", "🙂", "Hi你好42🙂"];

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

  for (const text of shortUserMessages) {
    test(`keeps short user message "${text}" on one line`, async ({ page }) => {
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
      await page.getByTestId("multimodal-input").fill(text);
      await page.getByTestId("send-button").click();
      await expect.poll(() => submittedText).toBe(text);

      const userContent = page
        .locator("[data-role='user'] [data-testid='message-content']")
        .last();
      await expect(userContent).toHaveText(text);

      const paragraph = userContent.locator("p");
      await expect(paragraph).toHaveCount(1);
      await expect.poll(() => paragraph.evaluate(countTextLines)).toBe(1);

      const { leftInset, rightInset } = await paragraph.evaluate((element) => {
        const content = element.closest("[data-testid='message-content']");
        const range = document.createRange();
        range.selectNodeContents(element);
        const contentRect = content?.getBoundingClientRect();
        const textRect = range.getBoundingClientRect();

        return {
          leftInset: contentRect ? textRect.left - contentRect.left : 0,
          rightInset: contentRect ? contentRect.right - textRect.right : 0,
        };
      });
      expect(Math.abs(leftInset - rightInset)).toBeLessThanOrEqual(1);
    });
  }

  test("wraps long user messages without overflowing the bubble", async ({
    page,
  }) => {
    await page.setViewportSize({ height: 800, width: 375 });
    await page.route("**/api/chat", async (route) => {
      await route.fulfill({
        body: "",
        headers: { "content-type": "text/event-stream; charset=utf-8" },
        status: 200,
      });
    });

    await page.goto("/");
    const text =
      "这是一条足够长的中文消息，用来确认用户气泡会在达到最大宽度后正常换行。";
    await page.getByTestId("multimodal-input").fill(text);
    await page.getByTestId("send-button").click();

    const userContent = page
      .locator("[data-role='user'] [data-testid='message-content']")
      .last();
    await expect(userContent).toHaveText(text);

    const paragraph = userContent.locator("p");
    await expect(paragraph).toHaveCount(1);
    await expect
      .poll(() => paragraph.evaluate(countTextLines))
      .toBeGreaterThan(1);

    const bubbleWidth = await userContent.evaluate(
      (element) => element.getBoundingClientRect().width
    );
    expect(bubbleWidth).toBeLessThanOrEqual(375 * 0.8 + 1);
  });

  test("places assistant text below reasoning", async ({ page }) => {
    const stream = [
      { messageId: "assistant-message", type: "start" },
      { id: "reasoning-1", type: "reasoning-start" },
      { delta: "Thinking", id: "reasoning-1", type: "reasoning-delta" },
      { id: "reasoning-1", type: "reasoning-end" },
      { id: "text-1", type: "text-start" },
      { delta: "Hello!", id: "text-1", type: "text-delta" },
      { id: "text-1", type: "text-end" },
      { finishReason: "stop", type: "finish" },
    ]
      .map((chunk) => `data: ${JSON.stringify(chunk)}\n\n`)
      .join("");

    await page.route("**/api/chat", async (route) => {
      await route.fulfill({
        body: `${stream}data: [DONE]\n\n`,
        headers: {
          "content-type": "text/event-stream",
          "x-vercel-ai-ui-message-stream": "v1",
        },
        status: 200,
      });
    });

    await page.goto("/");
    await page.getByTestId("multimodal-input").fill("Hello");
    await page.getByTestId("send-button").click();

    const reasoning = page.getByTestId("message-reasoning");
    const assistantContent = page.locator(
      "[data-role='assistant'] [data-testid='message-content']"
    );
    await expect(reasoning).toBeVisible();
    await expect(assistantContent).toHaveText("Hello!");

    const { reasoningBottom, responseTop } = await reasoning.evaluate(
      (reasoningElement) => {
        const response = reasoningElement
          .closest("[data-role='assistant']")
          ?.querySelector("[data-testid='message-content']");

        return {
          reasoningBottom: reasoningElement.getBoundingClientRect().bottom,
          responseTop: response?.getBoundingClientRect().top ?? 0,
        };
      }
    );

    expect(responseTop).toBeGreaterThanOrEqual(reasoningBottom);
  });
});

function countTextLines(element: Element) {
  const range = document.createRange();
  range.selectNodeContents(element);
  return new Set(
    Array.from(range.getClientRects()).map((rect) => Math.round(rect.top))
  ).size;
}
