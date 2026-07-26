export const isProductionEnvironment = process.env.NODE_ENV === "production";
export const isDevelopmentEnvironment = process.env.NODE_ENV === "development";
export const isTestEnvironment = Boolean(
  process.env.PLAYWRIGHT_TEST_BASE_URL ||
    process.env.PLAYWRIGHT ||
    process.env.CI_PLAYWRIGHT
);

// Resumable streams are opt-in until the durable resume endpoint is complete.
export const isResumableStreamsEnabled =
  process.env.RESUMABLE_STREAMS_ENABLED === "1";
export const isResumableStreamsClientEnabled =
  process.env.NEXT_PUBLIC_RESUMABLE_STREAMS_ENABLED === "1";

export const suggestions = [
  "What are the advantages of using Next.js?",
  "Write code to demonstrate Dijkstra's algorithm",
  "Help me write an essay about Silicon Valley",
  "What is the weather in San Francisco?",
];
