import { expect, test } from "@playwright/test";

const DEEPSEEK_V4_FLASH = "deepseek/deepseek-v4-flash";
const CHAT_ID_REGEX = /\/chat\/([\w-]+)/;

test("sends a real DeepSeek V4 Flash text chat", async ({ page }) => {
  let chatId: string | undefined;
  let cleanupError: string | undefined;
  let identityHeaders: Record<string, string> | undefined;

  try {
    const modelsResponsePromise = page.waitForResponse((response) =>
      response.url().includes("/api/models")
    );
    await page.goto("/");
    await modelsResponsePromise;

    const modelSelector = page.getByTestId("model-selector");
    await modelSelector.click();
    const modelSearch = page.getByPlaceholder("Search models...");
    await modelSearch.fill("DeepSeek V4 Flash");
    await modelSearch.press("Enter");

    const chatRequestPromise = page.waitForRequest(
      (request) =>
        request.url().includes("/api/chat") && request.method() === "POST"
    );
    const chatResponsePromise = page.waitForResponse(
      (response) =>
        response.url().includes("/api/chat") &&
        response.request().method() === "POST"
    );

    await page.getByTestId("multimodal-input").fill("hello");
    await page.getByTestId("send-button").click();

    const [chatRequest, chatResponse] = await Promise.all([
      chatRequestPromise,
      chatResponsePromise,
    ]);
    await chatResponse.finished();
    const requestHeaders = chatRequest.headers();
    identityHeaders = {
      "x-goog-authenticated-user-email":
        requestHeaders["x-goog-authenticated-user-email"] ?? "",
      "x-goog-authenticated-user-id":
        requestHeaders["x-goog-authenticated-user-id"] ?? "",
    };
    const requestBody = chatRequest.postDataJSON() as {
      id?: string;
      selectedChatModel?: string;
    };
    expect(requestBody.selectedChatModel).toBe(DEEPSEEK_V4_FLASH);
    expect(requestBody.id).toMatch(/^[\w-]+$/);
    chatId = requestBody.id;

    const assistantMessage = page.getByTestId("message-assistant");
    await expect(assistantMessage).toBeVisible({ timeout: 180_000 });
    await expect
      .poll(async () => (await assistantMessage.textContent())?.trim() ?? "", {
        timeout: 180_000,
      })
      .not.toBe("");

    await expect(page).toHaveURL(CHAT_ID_REGEX, { timeout: 30_000 });
    expect(page.url().match(CHAT_ID_REGEX)?.[1]).toBe(chatId);
  } finally {
    if (chatId) {
      const deleteResult = await page.evaluate(
        async ({ headers, id }) => {
          const response = await fetch(`/api/chat?id=${id}`, {
            headers,
            method: "DELETE",
          });
          return { body: await response.text(), status: response.status };
        },
        { headers: identityHeaders, id: chatId }
      );
      if (deleteResult.status < 200 || deleteResult.status >= 300) {
        cleanupError = `Generated chat cleanup failed for ${chatId} with HTTP ${deleteResult.status}: ${deleteResult.body}`;
      }
    }
  }

  if (cleanupError) {
    throw new Error(cleanupError);
  }
});
