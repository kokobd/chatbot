import assert from "node:assert/strict";
import test from "node:test";
import {
  createFallbackTitle,
  normalizeGeneratedTitle,
} from "../../lib/ai/title";

test("uses the user's text as a fallback title", () => {
  assert.equal(createFallbackTitle("  hi   there  "), "hi there");
  assert.equal(createFallbackTitle(""), "Image chat");
});

test("rejects generic generated titles", () => {
  assert.equal(normalizeGeneratedTitle("New Conversation", "Hi"), "Hi");
  assert.equal(normalizeGeneratedTitle('Title: "Hi"', "Fallback"), "Hi");
  assert.equal(normalizeGeneratedTitle("", "Fallback"), "Fallback");
});
