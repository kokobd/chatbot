import { expect, test } from "@playwright/test";

const chatId = "11111111-1111-4111-8111-111111111111";
const originalText = "Original message to edit";

test.describe("Message editing", () => {
  test.beforeEach(async ({ page }) => {
    await page.route("**/api/history**", async (route) => {
      await route.fulfill({
        body: JSON.stringify({
          chats: [],
          hasMore: false,
          nextCursor: null,
        }),
        contentType: "application/json",
        status: 200,
      });
    });

    await page.route("**/api/messages**", async (route) => {
      await route.fulfill({
        body: JSON.stringify({
          isReadonly: false,
          messages: [
            {
              id: "user-message",
              metadata: { createdAt: new Date().toISOString() },
              parts: [{ text: originalText, type: "text" }],
              role: "user",
            },
          ],
          userId: "test-user",
          visibility: "private",
        }),
        contentType: "application/json",
        status: 200,
      });
    });

    await page.goto(`/chat/${chatId}`);
    await expect(page.getByTestId("message-edit-button")).toHaveCount(1);
  });

  test("enters editing state with a focused input and can cancel", async ({
    page,
  }) => {
    await page.getByTestId("message-edit-button").click();

    const input = page.getByTestId("multimodal-input");
    const editingState = page.getByTestId("editing-message-state");
    const cancelButton = page.getByTestId("cancel-edit-button");

    await expect(input).toHaveValue(originalText);
    await expect(input).toBeFocused();
    await expect(editingState).toBeVisible();
    await expect(cancelButton).toBeVisible();
    await expect(cancelButton).toBeEnabled();

    await cancelButton.click();

    await expect(input).toHaveValue("");
    await expect(editingState).toHaveCount(0);
  });

  test("cancels editing with Escape", async ({ page }) => {
    await page.getByTestId("message-edit-button").click();

    const input = page.getByTestId("multimodal-input");
    await expect(page.getByTestId("editing-message-state")).toBeVisible();

    await input.press("Escape");

    await expect(input).toHaveValue("");
    await expect(page.getByTestId("editing-message-state")).toHaveCount(0);
  });
});
