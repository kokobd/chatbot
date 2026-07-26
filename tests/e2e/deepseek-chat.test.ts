import { expect, test } from "@playwright/test";

const DEEPSEEK_V4_FLASH = "deepseek/deepseek-v4-flash";
const CHAT_ID_REGEX = /\/chat\/([\w-]+)/;
const REQUEST_TIMEOUT = 30_000;

function testIdentityHeaders() {
  const email = process.env.E2E_IAP_TEST_EMAIL;
  const subject = process.env.E2E_IAP_TEST_SUBJECT;

  if (!email || !subject) {
    throw new Error("Playwright test identity was not initialized");
  }

  return {
    "x-goog-authenticated-user-email": `accounts.google.com:${email}`,
    "x-goog-authenticated-user-id": `accounts.google.com:${subject}`,
  };
}

test("sends a real DeepSeek V4 Flash text chat", async ({ page, request }) => {
  const identityHeaders = testIdentityHeaders();
  let chatId: string | undefined;
  let cleanupError: string | undefined;

  try {
    const modelsResponsePromise = page.waitForResponse(
      (response) =>
        response.url().includes("/api/models") &&
        response.request().method() === "GET",
      { timeout: REQUEST_TIMEOUT }
    );
    const pageResponse = await page.goto("/", {
      timeout: REQUEST_TIMEOUT,
      waitUntil: "domcontentloaded",
    });
    expect(pageResponse?.ok()).toBeTruthy();

    const modelsResponse = await modelsResponsePromise;
    expect(modelsResponse.status()).toBe(200);

    const modelSelector = page.getByTestId("model-selector");
    await modelSelector.click();
    const modelSearch = page.getByPlaceholder("Search models...");
    await modelSearch.fill("DeepSeek V4 Flash");
    const modelOption = page
      .locator('[role="option"]')
      .filter({ hasText: "DeepSeek V4 Flash" })
      .first();
    await expect(modelOption).toBeVisible({ timeout: REQUEST_TIMEOUT });
    await modelOption.click({ force: true });
    await expect(modelSelector).toContainText("DeepSeek V4 Flash");

    const chatRequestPromise = page.waitForRequest(
      (requestEvent) =>
        requestEvent.url().includes("/api/chat") &&
        requestEvent.method() === "POST",
      { timeout: REQUEST_TIMEOUT }
    );
    const chatResponsePromise = page.waitForResponse(
      (response) =>
        response.url().includes("/api/chat") &&
        response.request().method() === "POST",
      { timeout: REQUEST_TIMEOUT }
    );

    await page.getByTestId("multimodal-input").fill("hello");
    await page.getByTestId("send-button").click();

    // Capture the durable id as soon as the request is sent. Cleanup must not
    // depend on the provider stream completing successfully.
    const chatRequest = await chatRequestPromise;
    const requestBody = chatRequest.postDataJSON() as {
      id?: string;
      message?: { role?: string };
      selectedChatModel?: string;
    };
    expect(requestBody.selectedChatModel).toBe(DEEPSEEK_V4_FLASH);
    expect(requestBody.message?.role).toBe("user");
    expect(requestBody.id).toMatch(/^[\w-]+$/);
    chatId = requestBody.id;

    const chatResponse = await chatResponsePromise;
    expect(chatResponse.status()).toBe(200);
    expect(chatResponse.headers()["content-type"]).toContain(
      "text/event-stream"
    );
    await chatResponse.finished();

    const assistantMessage = page.getByTestId("message-assistant").last();
    await expect(assistantMessage).toBeVisible({ timeout: REQUEST_TIMEOUT });
    await expect
      .poll(async () => (await assistantMessage.textContent())?.trim() ?? "", {
        timeout: REQUEST_TIMEOUT,
      })
      .not.toBe("");

    await expect(page).toHaveURL(CHAT_ID_REGEX, { timeout: REQUEST_TIMEOUT });
    expect(page.url().match(CHAT_ID_REGEX)?.[1]).toBe(chatId);

    if (!chatId) {
      throw new Error("Chat request did not contain an id");
    }
    const messagesUrl = `/api/messages?chatId=${encodeURIComponent(chatId)}`;
    await expect
      .poll(
        async () => {
          const response = await request.get(messagesUrl, {
            headers: identityHeaders,
          });
          if (!response.ok()) {
            return `http:${response.status()}`;
          }
          const body = (await response.json()) as {
            messages?: Array<{
              id?: string;
              parts?: Array<{ text?: string }>;
              role?: string;
            }>;
          };
          const messages = body.messages ?? [];
          const assistantText = messages
            .filter((message) => message.role === "assistant")
            .flatMap((message) => message.parts ?? [])
            .map((part) => part.text ?? "")
            .join("")
            .trim();
          return `${messages.filter((message) => message.role === "user").length}:${assistantText.length}`;
        },
        { intervals: [250, 500, 1000, 2000], timeout: REQUEST_TIMEOUT }
      )
      .toMatch(/^1:[1-9]\d*$/);

    const persistedResponse = await request.get(messagesUrl, {
      headers: identityHeaders,
    });
    expect(persistedResponse.status()).toBe(200);
    const persisted = (await persistedResponse.json()) as {
      messages?: Array<{ id?: string }>;
    };
    const persistedIds = (persisted.messages ?? []).map(
      (message) => message.id
    );
    expect(new Set(persistedIds).size).toBe(persistedIds.length);
  } finally {
    if (chatId) {
      try {
        const deleteResult = await request.delete(`/api/chat?id=${chatId}`, {
          headers: identityHeaders,
          timeout: 10_000,
        });
        if (!deleteResult.ok()) {
          cleanupError = `Generated chat cleanup failed for ${chatId} with HTTP ${deleteResult.status()}: ${await deleteResult.text()}`;
        }
      } catch (error) {
        cleanupError = `Generated chat cleanup failed for ${chatId}: ${error instanceof Error ? error.message : String(error)}`;
      }
    }

    // The identity is unique to this runner invocation, so this fallback is
    // safe and also removes chats created before a request id was captured.
    try {
      const historyCleanup = await request.delete("/api/history", {
        headers: identityHeaders,
        timeout: 10_000,
      });
      if (!historyCleanup.ok() && !cleanupError) {
        cleanupError = `Test history cleanup failed with HTTP ${historyCleanup.status()}: ${await historyCleanup.text()}`;
      }
    } catch (error) {
      if (!cleanupError) {
        cleanupError = `Test history cleanup failed: ${error instanceof Error ? error.message : String(error)}`;
      }
    }
  }

  if (cleanupError) {
    throw new Error(cleanupError);
  }
});
