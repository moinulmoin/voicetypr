import { createInterface } from "node:readline";
import { stdin, stdout, stderr, exit } from "node:process";
import {
  completeSimple,
  getModel,
  getModels,
  getProviders,
  type Context,
  type AssistantMessage,
  type Model,
  type SimpleStreamOptions,
} from "@earendil-works/pi-ai";

type JsonValue = null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue };

type BaseRequest = {
  id?: string;
  protocolVersion?: number;
  type?: string;
};

type HealthRequest = BaseRequest & { type: "health" };
type ListProvidersRequest = BaseRequest & { type: "list_providers" };
type ListModelsRequest = BaseRequest & { type: "list_models"; provider: string };
type ShutdownRequest = BaseRequest & { type: "shutdown" };
type FormatRequest = BaseRequest & {
  type: "format";
  provider: string;
  model: string;
  prompt: string;
  systemPrompt?: string;
  apiKey?: string;
  noAuth?: boolean;
  customBaseUrl?: string;
  timeoutMs?: number;
};

type Request =
  | HealthRequest
  | ListProvidersRequest
  | ListModelsRequest
  | FormatRequest
  | ShutdownRequest;

const PROTOCOL_VERSION = 1;
const DEFAULT_TIMEOUT_MS = 30_000;
const RETRY_BACKOFF_MS = 500;
function writeResponse(response: Record<string, JsonValue>): void {
  stdout.write(`${JSON.stringify(response)}\n`);
}

function sanitizeError(error: unknown): string {
  const raw = error instanceof Error ? error.message : String(error);
  return raw
    .replace(/sk-ant-[A-Za-z0-9_-]+/g, "sk-ant-[REDACTED]")
    .replace(/sk-[A-Za-z0-9_-]+/g, "sk-[REDACTED]")
    .replace(/AIza[0-9A-Za-z_-]+/g, "AIza[REDACTED]")
    .replace(/xai-[A-Za-z0-9_-]+/g, "xai-[REDACTED]")
    .replace(/ghp_[A-Za-z0-9_]+/g, "ghp_[REDACTED]")
    .replace(/Bearer\s+[A-Za-z0-9._~+/=-]+/gi, "Bearer [REDACTED]")
    .slice(0, 500);
}

function classifyError(error: unknown): { code: string; retryable: boolean; message: string } {
  const message = sanitizeError(error);
  const haystack = message.toLowerCase();

  if (haystack.includes("rate limit") || haystack.includes("429")) {
    return { code: "rate_limited", retryable: true, message: "Rate limited" };
  }
  if (
    haystack.includes("unauthorized") ||
    haystack.includes("401") ||
    haystack.includes("invalid api key") ||
    haystack.includes("missing api key")
  ) {
    return { code: "auth_invalid", retryable: false, message: "Authentication failed" };
  }
  if (haystack.includes("timeout") || haystack.includes("aborted")) {
    return { code: "timeout", retryable: true, message: "Formatting timed out" };
  }
  if (
    haystack.includes("network") ||
    haystack.includes("fetch failed") ||
    haystack.includes("econn") ||
    haystack.includes("service unavailable") ||
    haystack.includes("server error") ||
    haystack.includes("internal server error") ||
    haystack.includes("overloaded") ||
    haystack.includes("capacity") ||
    haystack.includes("500") ||
    haystack.includes("503") ||
    haystack.includes("502") ||
    haystack.includes("504") ||
    haystack.includes("temporarily unavailable") ||
    haystack.includes("try again") ||
    haystack.includes("retryable")
  ) {
    return { code: "network", retryable: true, message: "Network error" };
  }
  if (
    haystack.includes("bad request") ||
    haystack.includes("400") ||
    haystack.includes("invalid prompt") ||
    haystack.includes("invalid request") ||
    haystack.includes("validation")
  ) {
    return { code: "provider_error", retryable: false, message };
  }
  if (haystack.includes("not found") || haystack.includes("unsupported") || haystack.includes("model")) {
    return { code: "unsupported_model", retryable: false, message };
  }

  return { code: "provider_error", retryable: false, message };
}

function textFromResponse(response: AssistantMessage): string {
  const blocks = response.content ?? [];
  const text = blocks
    .filter((block): block is { type: "text"; text: string } => block?.type === "text" && typeof block.text === "string")
    .map((block) => block.text)
    .join("")
    .trim();

  if (!text) {
    throw new Error("Provider returned no text content");
  }

  return text;
}

function assertSuccessfulResponse(response: AssistantMessage): void {
  if (response.stopReason !== "stop") {
    throw new Error(response.errorMessage || `Provider stopped with ${response.stopReason}`);
  }
}

function buildCustomModel(request: FormatRequest): Model<any> {
  const baseUrl = request.customBaseUrl?.trim() || "http://localhost:11434/v1";
  return {
    id: request.model,
    name: request.model,
    api: "openai-completions",
    provider: "custom",
    baseUrl,
    reasoning: false,
    input: ["text"],
    cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
    contextWindow: 128_000,
    maxTokens: 16_384,
    compat: {
      supportsStore: false,
      supportsDeveloperRole: false,
      supportsReasoningEffort: false,
      supportsUsageInStreaming: false,
      maxTokensField: "max_tokens",
    },
  };
}

function resolveModel(request: FormatRequest): Model<any> {
  if (request.provider === "custom") {
    return buildCustomModel(request);
  }

  const model = getModel(request.provider as never, request.model as never);
  if (!model) {
    throw new Error(`Unsupported model: ${request.provider}/${request.model}`);
  }
  return model as Model<any>;
}

async function withTimeout<T>(promise: Promise<T>, timeoutMs: number): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new Error("Operation timed out")), timeoutMs);
  });

  try {
    return await Promise.race([promise, timeout]);
  } finally {
    if (timer) clearTimeout(timer);
  }
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function completeSimpleWithRetry(
  model: Model<any>,
  request: Context,
  options: Omit<SimpleStreamOptions, "signal" | "timeoutMs">,
  timeoutMs: number,
): Promise<AssistantMessage> {
  const deadline = Date.now() + timeoutMs;
  let retryAvailable = true;

  for (;;) {
    const remainingMs = Math.max(0, deadline - Date.now());
    if (remainingMs <= 0) {
      throw new Error("Operation timed out");
    }

    try {
      return await withTimeout(
        completeSimple(model, request, {
          ...options,
          signal: AbortSignal.timeout(remainingMs),
          timeoutMs: remainingMs,
        }),
        remainingMs,
      );
    } catch (error) {
      const remainingAfterFailureMs = deadline - Date.now();
      if (!retryAvailable || !classifyError(error).retryable || remainingAfterFailureMs <= RETRY_BACKOFF_MS) {
        throw error;
      }

      retryAvailable = false;
      await sleep(RETRY_BACKOFF_MS);
    }
  }
}

function resolveApiKey(request: FormatRequest): string {
  const apiKey = request.apiKey?.trim();
  if (request.noAuth) {
    return apiKey || "dummy";
  }
  if (!apiKey) {
    throw new Error(`Missing API key for provider: ${request.provider}`);
  }
  return apiKey;
}

function formatMaxTokens(prompt: string, model: Model<any>): number {
  const modelLimit = typeof model.maxTokens === "number" && model.maxTokens > 0 ? model.maxTokens : 4096;
  const promptScaledLimit = Math.max(512, Math.ceil(prompt.length / 2));
  return Math.min(modelLimit, 4096, promptScaledLimit);
}

async function handleFormat(request: FormatRequest): Promise<Record<string, JsonValue>> {
  if (!request.prompt || !request.prompt.trim()) {
    throw new Error("Prompt cannot be empty");
  }

  const started = Date.now();
  const model = resolveModel(request);
  const timeoutMs = Math.min(Math.max(request.timeoutMs ?? DEFAULT_TIMEOUT_MS, 1_000), 120_000);
  const apiKey = resolveApiKey(request);
  const options: Omit<SimpleStreamOptions, "signal" | "timeoutMs"> = {
    apiKey,
    maxRetries: 0,
    maxRetryDelayMs: 0,
    maxTokens: formatMaxTokens(request.prompt, model),
  };

  const response = await completeSimpleWithRetry(
    model,
    {
      systemPrompt:
        request.systemPrompt ||
        "You are a careful text formatter. Return only the cleaned text requested by the user instructions.",
      messages: [{ role: "user", content: request.prompt, timestamp: Date.now() }],
    },
    options,
    timeoutMs,
  );

  assertSuccessfulResponse(response);
  const text = textFromResponse(response);
  const usage = response.usage;

  return {
    id: request.id ?? null,
    protocolVersion: PROTOCOL_VERSION,
    ok: true,
    type: "formatted",
    text,
    provider: String((model as any).provider ?? request.provider),
    model: String((model as any).id ?? request.model),
    latencyMs: Date.now() - started,
    usage: usage
      ? {
          inputTokens: typeof usage.input === "number" ? usage.input : null,
          outputTokens: typeof usage.output === "number" ? usage.output : null,
          cost: typeof usage.cost?.total === "number" ? usage.cost.total : null,
        }
      : null,
  };
}

function handleListProviders(request: ListProvidersRequest): Record<string, JsonValue> {
  const providers: Array<{ id: string; name: string }> = getProviders().map((provider) => ({ id: provider, name: provider }));
  providers.push({ id: "custom", name: "Custom (OpenAI-compatible)" });
  return {
    id: request.id ?? null,
    protocolVersion: PROTOCOL_VERSION,
    ok: true,
    type: "providers",
    providers,
  };
}

function handleListModels(request: ListModelsRequest): Record<string, JsonValue> {
  if (request.provider === "custom") {
    return {
      id: request.id ?? null,
      protocolVersion: PROTOCOL_VERSION,
      ok: true,
      type: "models",
      provider: request.provider,
      models: [],
    };
  }

  const models = getModels(request.provider as never).map((model: any) => ({
    id: String(model.id),
    name: String(model.name || model.id),
    recommended: false,
    contextWindow: typeof model.contextWindow === "number" ? model.contextWindow : null,
    maxTokens: typeof model.maxTokens === "number" ? model.maxTokens : null,
    reasoning: Boolean(model.reasoning),
    input: Array.isArray(model.input) ? model.input.map(String) : [],
    provider: String(model.provider || request.provider),
  }));

  return {
    id: request.id ?? null,
    protocolVersion: PROTOCOL_VERSION,
    ok: true,
    type: "models",
    provider: request.provider,
    models,
  };
}

async function handleRequest(request: Request): Promise<Record<string, JsonValue>> {
  if (request.protocolVersion !== PROTOCOL_VERSION) {
    throw new Error(`Unsupported protocol version: ${request.protocolVersion}`);
  }

  switch (request.type) {
    case "health":
      return {
        id: request.id ?? null,
        protocolVersion: PROTOCOL_VERSION,
        ok: true,
        type: "ready",
      };
    case "list_providers":
      return handleListProviders(request);
    case "list_models":
      return handleListModels(request);
    case "format":
      return handleFormat(request);
    case "shutdown":
      return {
        id: request.id ?? null,
        protocolVersion: PROTOCOL_VERSION,
        ok: true,
        type: "shutdown",
      };
    default:
      throw new Error(`Unknown command type: ${(request as BaseRequest).type}`);
  }
}

const rl = createInterface({ input: stdin, crlfDelay: Infinity });

rl.on("line", (line) => {
  void (async () => {
    const trimmed = line.trim();
    if (!trimmed) return;

    let parsed: Request;
    try {
      parsed = JSON.parse(trimmed) as Request;
    } catch {
      writeResponse({ id: null, protocolVersion: PROTOCOL_VERSION, ok: false, code: "invalid_json", message: "Invalid JSON", retryable: false });
      return;
    }

    try {
      const response = await handleRequest(parsed);
      writeResponse(response);
      if (parsed.type === "shutdown") {
        rl.close();
        exit(0);
      }
    } catch (error) {
      const classified = classifyError(error);
      writeResponse({
        id: parsed.id ?? null,
        protocolVersion: PROTOCOL_VERSION,
        ok: false,
        type: "error",
        ...classified,
      });
    }
  })().catch((error) => {
    stderr.write(`Formatting sidecar fatal handler error: ${sanitizeError(error)}\n`);
  });
});

rl.on("close", () => {
  exit(0);
});
