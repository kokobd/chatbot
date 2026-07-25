import { APICallError } from "ai";

export type AIErrorCategory =
  | "credits"
  | "authentication"
  | "rate_limit"
  | "provider_unavailable"
  | "invalid_request"
  | "unknown";

export type ClassifiedAIError = {
  category: AIErrorCategory;
  message: string;
  statusCode?: number;
  retryable: boolean;
  diagnosticMessage: string;
};

const MAX_DIAGNOSTIC_LENGTH = 240;
const MAX_SIGNAL_LENGTH = 12_000;

const messages: Record<AIErrorCategory, string> = {
  authentication:
    "The model provider rejected the request credentials. Check the API key configuration, then try again.",
  credits:
    "OpenRouter does not have enough credits for this request. Increase the key limit or use a smaller output limit, then try again.",
  invalid_request:
    "The model or request is invalid. Check the selected model and try again.",
  provider_unavailable:
    "The model provider is temporarily unavailable. Please try again.",
  rate_limit:
    "The model provider is rate limiting requests. Please wait a moment and try again.",
  unknown: "We couldn't complete this request. Please try again.",
};

type ErrorLike = {
  cause?: unknown;
  data?: unknown;
  error?: unknown;
  isRetryable?: unknown;
  message?: unknown;
  responseBody?: unknown;
  status?: unknown;
  statusCode?: unknown;
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function getStatusCode(error: ErrorLike): number | undefined {
  const status = error.statusCode ?? error.status;
  const statusCode =
    typeof status === "number"
      ? status
      : typeof status === "string" && /^\d{3}$/.test(status)
        ? Number(status)
        : undefined;

  return statusCode && statusCode >= 100 && statusCode <= 599
    ? statusCode
    : undefined;
}

function getNestedErrors(error: unknown): ErrorLike[] {
  const result: ErrorLike[] = [];
  const queue: unknown[] = [error];
  const visited = new Set<object>();

  while (queue.length > 0 && result.length < 20) {
    const current = queue.shift();
    if (!isRecord(current)) {
      continue;
    }
    if (visited.has(current)) {
      continue;
    }

    visited.add(current);
    result.push(current);

    for (const nested of [current.error, current.cause]) {
      if (isRecord(nested)) {
        queue.push(nested);
      }
    }
  }

  return result;
}

function getSignalText(errors: ErrorLike[]): string {
  const values: string[] = [];

  for (const error of errors) {
    for (const value of [error.message, error.responseBody, error.data]) {
      if (typeof value === "string") {
        values.push(value);
      } else if (value !== undefined) {
        try {
          values.push(JSON.stringify(value));
        } catch {
          // Some provider error objects are not serializable. Their message is
          // sufficient for classification in that case.
        }
      }
    }
  }

  return values.join(" ").slice(0, MAX_SIGNAL_LENGTH).toLowerCase();
}

function getDiagnosticMessage(errors: ErrorLike[]): string {
  const message = errors
    .map((error) => error.message)
    .find(
      (value): value is string => typeof value === "string" && value.length > 0
    );

  if (!message) {
    return "unknown provider error";
  }

  return message
    .replace(/https?:\/\/\S+/gi, "[url]")
    .replace(/\b(?:bearer\s+)?[a-z0-9_-]*key[a-z0-9_=-]*\b/gi, "[credential]")
    .replace(/\b(?:sk|key|token|secret)[-_][a-z0-9_-]{12,}\b/gi, "[credential]")
    .replace(/\{[\s\S]*\}/g, "[provider details]")
    .replace(/\s+/g, " ")
    .trim()
    .slice(0, MAX_DIAGNOSTIC_LENGTH);
}

function classifyCategory(
  signalText: string,
  statusCode: number | undefined
): AIErrorCategory {
  if (
    statusCode === 402 ||
    (/openrouter/.test(signalText) &&
      /(credit|balance|billing|payment|insufficient|limit)/.test(signalText))
  ) {
    return "credits";
  }

  if (
    statusCode === 401 ||
    statusCode === 403 ||
    /(authentication|unauthorized|invalid api key|api key|access token|credential|forbidden)/.test(
      signalText
    )
  ) {
    return "authentication";
  }

  if (
    statusCode === 429 ||
    /(rate limit|rate-limit|too many requests|throttl)/.test(signalText)
  ) {
    return "rate_limit";
  }

  if (
    (statusCode !== undefined && statusCode >= 500) ||
    statusCode === 408 ||
    /(temporarily unavailable|service unavailable|overloaded|timeout|timed out|connection refused|econnreset|network error|fetch failed|socket|dns)/.test(
      signalText
    )
  ) {
    return "provider_unavailable";
  }

  if (
    statusCode === 400 ||
    statusCode === 404 ||
    statusCode === 422 ||
    /(invalid (?:model|request|parameter)|model not found|no such model|unknown model|context length|malformed|bad request)/.test(
      signalText
    )
  ) {
    return "invalid_request";
  }

  return "unknown";
}

export function classifyAIError(error: unknown): ClassifiedAIError {
  const errors = getNestedErrors(error);
  const apiError = errors.find((candidate) =>
    APICallError.isInstance(candidate)
  );
  const statusCode = apiError
    ? getStatusCode(apiError)
    : errors.map(getStatusCode).find((status) => status !== undefined);
  const signalText = getSignalText(errors);
  const category = classifyCategory(signalText, statusCode);
  const retryable =
    apiError && typeof apiError.isRetryable === "boolean"
      ? apiError.isRetryable === true
      : category === "rate_limit" || category === "provider_unavailable";

  return {
    category,
    diagnosticMessage: getDiagnosticMessage(errors),
    message: messages[category],
    ...(statusCode === undefined ? {} : { statusCode }),
    retryable,
  };
}
