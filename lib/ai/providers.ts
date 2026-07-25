import { createOpenRouter } from "@openrouter/ai-sdk-provider";
import { titleModel } from "./models";

const openrouter = createOpenRouter({
  apiKey: process.env.OPENROUTER_API_KEY,
  headers: {
    ...(process.env.OPENROUTER_HTTP_REFERER && {
      "HTTP-Referer": process.env.OPENROUTER_HTTP_REFERER,
    }),
    ...(process.env.OPENROUTER_APP_NAME && {
      "X-Title": process.env.OPENROUTER_APP_NAME,
    }),
  },
});

export function getLanguageModel(modelId: string) {
  return openrouter.chat(modelId);
}

export function getTitleModel() {
  return openrouter.chat(titleModel.id);
}
