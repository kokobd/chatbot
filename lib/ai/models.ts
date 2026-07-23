import { isTestEnvironment } from "../constants";

const OPENROUTER_API_BASE_URL = "https://openrouter.ai/api/v1";

export type ModelCapabilities = {
  tools: boolean;
  vision: boolean;
  reasoning: boolean;
};

export type ChatModel = {
  id: string;
  name: string;
  provider: string;
  description: string;
  capabilities: ModelCapabilities;
};

export const DEFAULT_CHAT_MODEL = "moonshotai/kimi-k3";

export const titleModel = {
  description: "Fast model for title generation",
  id: "~google/gemini-flash-latest",
  name: "Gemini Flash Latest",
  provider: "google",
};

export const chatModels: ChatModel[] = [
  {
    capabilities: { reasoning: true, tools: true, vision: true },
    description: "Multimodal reasoning model for complex work and coding",
    id: "moonshotai/kimi-k3",
    name: "Kimi K3",
    provider: "moonshotai",
  },
  {
    capabilities: { reasoning: true, tools: true, vision: false },
    description: "Large-scale reasoning model for coding and agent workflows",
    id: "deepseek/deepseek-v4-pro",
    name: "DeepSeek V4 Pro",
    provider: "deepseek",
  },
  {
    capabilities: { reasoning: true, tools: true, vision: false },
    description: "Large-scale reasoning model from Z.ai",
    id: "z-ai/glm-5.2",
    name: "GLM 5.2",
    provider: "zai",
  },
  {
    capabilities: { reasoning: false, tools: true, vision: true },
    description: "OpenAI's latest ChatGPT instant model",
    id: "openai/gpt-chat-latest",
    name: "ChatGPT Latest",
    provider: "openai",
  },
  {
    capabilities: { reasoning: true, tools: true, vision: true },
    description: "Anthropic's latest Sonnet model",
    id: "~anthropic/claude-sonnet-latest",
    name: "Claude Sonnet Latest",
    provider: "anthropic",
  },
  {
    capabilities: { reasoning: true, tools: true, vision: true },
    description: "Google's latest fast multimodal model",
    id: "~google/gemini-flash-latest",
    name: "Gemini Flash Latest",
    provider: "google",
  },
];

type OpenRouterModel = {
  architecture?: {
    input_modalities?: string[];
    output_modalities?: string[];
  };
  canonical_slug?: string;
  id: string;
  name: string;
  supported_parameters?: string[];
};

type OpenRouterEndpoint = {
  latency_last_1h?: {
    p50?: number;
    p95?: number;
  };
  provider_name?: string;
  status?: number;
  uptime_last_15m?: number;
  uptime_last_1h?: number;
};

function openRouterHeaders() {
  const headers = new Headers();
  const apiKey = process.env.OPENROUTER_API_KEY;

  if (apiKey) {
    headers.set("Authorization", `Bearer ${apiKey}`);
  }

  if (process.env.OPENROUTER_HTTP_REFERER) {
    headers.set("HTTP-Referer", process.env.OPENROUTER_HTTP_REFERER);
  }

  if (process.env.OPENROUTER_APP_NAME) {
    headers.set("X-Title", process.env.OPENROUTER_APP_NAME);
  }

  return headers;
}

function normalizedModelId(modelId: string) {
  return modelId.replace(/^~/, "");
}

function modelEndpointUrl(modelId: string) {
  const [author, ...slugParts] = normalizedModelId(modelId).split("/");
  return `${OPENROUTER_API_BASE_URL}/models/${author}/${slugParts.join("/")}/endpoints`;
}

async function getOpenRouterModels(): Promise<OpenRouterModel[]> {
  const res = await fetch(`${OPENROUTER_API_BASE_URL}/models`, {
    headers: openRouterHeaders(),
    next: { revalidate: 86_400 },
  });

  if (!res.ok) {
    return [];
  }

  const json = (await res.json()) as { data?: OpenRouterModel[] };
  return json.data ?? [];
}

function getModelMetadata(
  models: OpenRouterModel[],
  modelId: string
): OpenRouterModel | undefined {
  const normalizedId = normalizedModelId(modelId);
  return models.find(
    (model) =>
      model.id === modelId ||
      model.id === normalizedId ||
      model.canonical_slug === normalizedId
  );
}

function getModelCapabilities(
  model: OpenRouterModel | undefined,
  fallback: ModelCapabilities
): ModelCapabilities {
  if (!model) {
    return fallback;
  }

  const parameters = new Set(model.supported_parameters ?? []);
  const inputModalities = new Set(model.architecture?.input_modalities ?? []);

  return {
    reasoning: parameters.has("reasoning"),
    tools: parameters.has("tools"),
    vision: inputModalities.has("image"),
  };
}

export async function getCapabilities(): Promise<
  Record<string, ModelCapabilities>
> {
  const fallback = Object.fromEntries(
    chatModels.map((model) => [model.id, model.capabilities])
  );

  if (isTestEnvironment) {
    return fallback;
  }

  try {
    const models = await getOpenRouterModels();
    return Object.fromEntries(
      chatModels.map((model) => [
        model.id,
        getModelCapabilities(
          getModelMetadata(models, model.id),
          model.capabilities
        ),
      ])
    );
  } catch {
    return fallback;
  }
}

export const isDemo = process.env.IS_DEMO === "1";

export type OpenRouterModelWithCapabilities = ChatModel & {
  capabilities: ModelCapabilities;
};

export async function getAllOpenRouterModels(): Promise<
  OpenRouterModelWithCapabilities[]
> {
  try {
    const models = await getOpenRouterModels();

    return models
      .filter((model) =>
        model.architecture?.output_modalities?.includes("text")
      )
      .map((model) => ({
        capabilities: getModelCapabilities(model, {
          reasoning: false,
          tools: false,
          vision: false,
        }),
        description: "",
        id: model.id,
        name: model.name,
        provider: model.id.split("/")[0],
      }));
  } catch {
    return [];
  }
}

export function getActiveModels(): ChatModel[] {
  return chatModels;
}

export const allowedModelIds = new Set(chatModels.map((model) => model.id));

export const modelsByProvider = chatModels.reduce(
  (acc, model) => {
    if (!acc[model.provider]) {
      acc[model.provider] = [];
    }
    acc[model.provider].push(model);
    return acc;
  },
  {} as Record<string, ChatModel[]>
);

export type ModelAvailability = "healthy" | "impacted" | "unknown";

const PROVIDER_IMPACTED_UPTIME_THRESHOLD = 99;
const PROVIDER_IMPACTED_P50_MS = 10_000;
const PROVIDER_IMPACTED_P95_MS = 30_000;

function isEndpointImpacted(endpoint: OpenRouterEndpoint) {
  return (
    (endpoint.status !== undefined && endpoint.status !== 0) ||
    (endpoint.uptime_last_15m !== undefined &&
      endpoint.uptime_last_15m < PROVIDER_IMPACTED_UPTIME_THRESHOLD) ||
    (endpoint.uptime_last_1h !== undefined &&
      endpoint.uptime_last_1h < PROVIDER_IMPACTED_UPTIME_THRESHOLD) ||
    (endpoint.latency_last_1h?.p50 !== undefined &&
      endpoint.latency_last_1h.p50 > PROVIDER_IMPACTED_P50_MS) ||
    (endpoint.latency_last_1h?.p95 !== undefined &&
      endpoint.latency_last_1h.p95 > PROVIDER_IMPACTED_P95_MS)
  );
}

export async function getModelAvailability(
  modelId: string
): Promise<ModelAvailability> {
  const model = chatModels.find((item) => item.id === modelId);

  if (!model) {
    return "unknown";
  }

  try {
    const res = await fetch(modelEndpointUrl(model.id), {
      headers: openRouterHeaders(),
      next: { revalidate: 60 },
    });

    if (!res.ok) {
      return "unknown";
    }

    const json = (await res.json()) as {
      data?: { endpoints?: OpenRouterEndpoint[] };
    };
    const endpoints = json.data?.endpoints ?? [];

    if (endpoints.length === 0) {
      return "unknown";
    }

    return endpoints.some(isEndpointImpacted) ? "impacted" : "healthy";
  } catch {
    return "unknown";
  }
}
