import { strict as assert } from "node:assert/strict";
import test from "node:test";
import { APICallError } from "ai";
import { classifyAIError } from "./error-classifier";

function apiError(
  statusCode: number,
  message = "provider request failed",
  responseBody = ""
) {
  return new APICallError({
    message,
    requestBodyValues: {
      api_key: "sk-or-v1-secret-token",
      messages: [{ content: "private prompt" }],
    },
    responseBody,
    statusCode,
    url: "https://openrouter.ai/api/v1/chat/completions",
  });
}

test("classifies a nested OpenRouter insufficient-credit error", () => {
  const result = classifyAIError({
    error: apiError(
      402,
      "Provider returned an error",
      '{"error":{"message":"This request exceeds your OpenRouter credits"}}'
    ),
  });

  assert.equal(result.category, "credits");
  assert.match(result.message, /does not have enough credits/);
  assert.equal(result.statusCode, 402);
  assert.equal(result.retryable, false);
});

test("classifies authentication, rate-limit, availability, and invalid-request errors", () => {
  assert.equal(classifyAIError(apiError(401)).category, "authentication");
  assert.equal(classifyAIError(apiError(403)).category, "authentication");
  assert.equal(classifyAIError(apiError(429)).category, "rate_limit");
  assert.equal(classifyAIError(apiError(503)).category, "provider_unavailable");
  assert.equal(
    classifyAIError(apiError(400, "The selected model is invalid")).category,
    "invalid_request"
  );
});

test("classifies unrecognized failures without exposing provider details", () => {
  const result = classifyAIError({
    cause: apiError(
      499,
      "request failed with token sk-or-v1-12345678901234567890; manage it at https://openrouter.ai/settings/keys?token=secret",
      `{"prompt":"private prompt","url":"https://example.com/key=secret"}${"x".repeat(5000)}`
    ),
  });

  assert.equal(result.category, "unknown");
  assert.doesNotMatch(JSON.stringify(result), /openrouter\.ai/);
  assert.doesNotMatch(result.diagnosticMessage, /sk-or-v1/);
  assert.doesNotMatch(result.diagnosticMessage, /https?:\/\//);
  assert.doesNotMatch(result.diagnosticMessage, /private prompt/);
  assert.doesNotMatch(result.diagnosticMessage, /secret/);
  assert.ok(result.diagnosticMessage.length <= 240);
});
