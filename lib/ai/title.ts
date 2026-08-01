const MAX_FALLBACK_TITLE_LENGTH = 80;

const GENERIC_TITLES = new Set([
  "chat",
  "new chat",
  "new conversation",
  "untitled",
]);

function cleanTitle(title: string) {
  return title
    .replace(/^\s*(?:title\s*:\s*)?/i, "")
    .replace(/^[#*"'`\s]+|[#*"'`\s]+$/g, "")
    .replace(/\s+/g, " ")
    .trim();
}

export function createFallbackTitle(text: string) {
  const normalized = text.replace(/\s+/g, " ").trim();

  if (!normalized) {
    return "Image chat";
  }

  return normalized.slice(0, MAX_FALLBACK_TITLE_LENGTH).trimEnd();
}

export function normalizeGeneratedTitle(title: string, fallback: string) {
  const cleaned = cleanTitle(title);
  const canonical = cleaned.toLocaleLowerCase().replace(/[.!?]+$/, "");

  if (!cleaned || GENERIC_TITLES.has(canonical)) {
    return fallback;
  }

  return cleaned;
}
