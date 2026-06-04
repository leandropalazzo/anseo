/**
 * @opengeo/observe — thin instrumentation SDK for the Run-Ingestion API.
 *
 * The OpenTelemetry pattern, minus the ceremony: you already ran a prompt
 * against an LLM provider *outside* OpenGEO. This SDK lets you POST that run
 * to `POST /v1/ingest/run` in one call, so it flows through the same
 * extraction → redaction → benchmark-contribution path as a native run.
 *
 * Zero runtime dependencies — it uses the global `fetch` (Node >= 18, Deno,
 * browsers, edge runtimes).
 *
 * ```ts
 * import { OpenGeoObserver } from "@opengeo/observe";
 *
 * const observer = new OpenGeoObserver({
 *   baseUrl: "https://opengeo.internal",
 *   apiKey: process.env.OPENGEO_API_KEY!,
 *   project: "Sunski",
 * });
 *
 * const result = await observer.observeRun({
 *   promptSlug: "best-polarized-sunglasses",
 *   provider: "openai",
 *   model: "gpt-4o-2024-08-06",
 *   responseText: completion.choices[0].message.content ?? "",
 * });
 * ```
 */

/** Configuration for an {@link OpenGeoObserver}. */
export interface OpenGeoObserverConfig {
  /** Base URL of the OpenGEO API, e.g. `https://opengeo.internal`. */
  baseUrl: string;
  /** API key, sent as the `X-OpenGEO-API-Key` header. */
  apiKey: string;
  /**
   * Project to scope the run to, sent as the `X-OpenGEO-Project` header
   * (resolved by brand name server-side). Optional for single-project
   * deployments that rely on the sole-active-project fallback.
   */
  project?: string;
  /** Request timeout in milliseconds. Defaults to 30000. */
  timeoutMs?: number;
  /**
   * Custom fetch implementation. Defaults to the global `fetch`. Useful for
   * tests or for injecting a proxy/retry-wrapped fetch.
   */
  fetch?: typeof fetch;
}

/** One externally-executed run to record. Mirrors the API's `IngestRunRequest`. */
export interface ObserveRunInput {
  /** Declared prompt slug within the project. Must already exist server-side. */
  promptSlug: string;
  /** Provider that produced the run, e.g. `"openai"`. */
  provider: string;
  /** Provider model version, e.g. `"gpt-4o-2024-08-06"`. */
  model: string;
  /**
   * Raw response text the provider returned. Optional when you already
   * extracted `citationDomains` yourself.
   */
  responseText?: string;
  /**
   * Source domains observed in the run's citations. When omitted and
   * `responseText` is set, the server extracts domains from the text.
   */
  citationDomains?: string[];
  /** The brand's observed rank in this run, if you computed it. */
  observedRank?: number;
  /** When the run was observed. Defaults to server-side `now` if omitted. */
  observedAt?: Date | string;
}

/**
 * Outcome of the benchmark-contribution leg. Mirrors the API's
 * `ContributionStatus` (an internally-tagged enum keyed on `status`).
 */
export type ContributionStatus =
  | { status: "sealed" }
  | { status: "skipped_not_opted_in" }
  | { status: "kek_missing" }
  | { status: "redaction_rejected"; reason: string };

/** Response from `POST /v1/ingest/run`. Mirrors the API's `IngestRunResponse`. */
export interface ObserveRunResult {
  runId: string;
  projectId: string;
  promptSlug: string;
  provider: string;
  observedAt: string;
  contribution: ContributionStatus;
}

/** Raised when the API returns a non-2xx response. */
export class OpenGeoApiError extends Error {
  /** HTTP status code. */
  readonly status: number;
  /** Machine-readable error code from the API body, if present. */
  readonly code: string | undefined;

  constructor(status: number, code: string | undefined, message: string) {
    super(message);
    this.name = "OpenGeoApiError";
    this.status = status;
    this.code = code;
  }
}

function toIso(value: Date | string | undefined): string | undefined {
  if (value === undefined) return undefined;
  return value instanceof Date ? value.toISOString() : value;
}

/**
 * Maps the camelCase SDK input to the snake_case wire shape the API expects,
 * omitting fields that were not provided so server-side defaults apply.
 */
function toWire(input: ObserveRunInput): Record<string, unknown> {
  const body: Record<string, unknown> = {
    prompt_slug: input.promptSlug,
    provider: input.provider,
    model: input.model,
  };
  if (input.responseText !== undefined) body.response_text = input.responseText;
  if (input.citationDomains !== undefined) {
    body.citation_domains = input.citationDomains;
  }
  if (input.observedRank !== undefined) body.observed_rank = input.observedRank;
  const observedAt = toIso(input.observedAt);
  if (observedAt !== undefined) body.observed_at = observedAt;
  return body;
}

/** Thin client around `POST /v1/ingest/run`. */
export class OpenGeoObserver {
  private readonly baseUrl: string;
  private readonly apiKey: string;
  private readonly project?: string;
  private readonly timeoutMs: number;
  private readonly fetchImpl: typeof fetch;

  constructor(config: OpenGeoObserverConfig) {
    if (!config.baseUrl) {
      throw new Error("OpenGeoObserver: `baseUrl` is required");
    }
    if (!config.apiKey) {
      throw new Error("OpenGeoObserver: `apiKey` is required");
    }
    // Normalize trailing slash so URL joining is unambiguous.
    this.baseUrl = config.baseUrl.replace(/\/+$/, "");
    this.apiKey = config.apiKey;
    this.project = config.project;
    this.timeoutMs = config.timeoutMs ?? 30_000;
    const f = config.fetch ?? globalThis.fetch;
    if (typeof f !== "function") {
      throw new Error(
        "OpenGeoObserver: no `fetch` available — pass `fetch` in the config or run on a platform that provides a global fetch (Node >= 18).",
      );
    }
    this.fetchImpl = f;
  }

  /**
   * Record one externally-executed run. Resolves with the parsed
   * {@link ObserveRunResult}; rejects with {@link OpenGeoApiError} on a non-2xx
   * response.
   */
  async observeRun(input: ObserveRunInput): Promise<ObserveRunResult> {
    const headers: Record<string, string> = {
      "content-type": "application/json",
      "x-opengeo-api-key": this.apiKey,
    };
    if (this.project) headers["x-opengeo-project"] = this.project;

    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), this.timeoutMs);
    let response: Response;
    try {
      response = await this.fetchImpl(`${this.baseUrl}/v1/ingest/run`, {
        method: "POST",
        headers,
        body: JSON.stringify(toWire(input)),
        signal: controller.signal,
      });
    } finally {
      clearTimeout(timer);
    }

    const text = await response.text();
    let parsed: unknown;
    try {
      parsed = text ? JSON.parse(text) : undefined;
    } catch {
      parsed = undefined;
    }

    if (!response.ok) {
      const body = parsed as { error?: string; message?: string } | undefined;
      throw new OpenGeoApiError(
        response.status,
        body?.error,
        body?.message ??
          `OpenGEO ingest failed: HTTP ${response.status}${text ? ` — ${text}` : ""}`,
      );
    }

    return parsed as ObserveRunResult;
  }
}

/**
 * One-shot convenience: construct an observer and record a single run.
 * Prefer reusing an {@link OpenGeoObserver} when sending many runs.
 */
export async function observeRun(
  config: OpenGeoObserverConfig,
  input: ObserveRunInput,
): Promise<ObserveRunResult> {
  return new OpenGeoObserver(config).observeRun(input);
}
