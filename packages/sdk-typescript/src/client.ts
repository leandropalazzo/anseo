/**
 * `fetch`-based client for `POST /v1/ingest/run` (Story 40.3).
 *
 * This is the **instrumentation** client: a thin, best-effort, *at-most-once*
 * sender that ships externally-executed LLM runs to the Anseo Run-Ingestion API
 * (the client side of the 40.1 contract, `apps/api/src/routes/ingest.rs`). It
 * is the TypeScript port of the Python reference (`anseo_observe`), against the
 * same language-agnostic spec in `docs/sdk-spec.md`.
 *
 * Two delivery surfaces, by design:
 *
 * - {@link AnseoObserver.observeRun} — **strict**. Resolves with the parsed
 *   {@link ObserveRunResult} and rejects with {@link AnseoApiError} on a non-2xx
 *   response. Use it for manual, synchronous control (e.g. a backfill that wants
 *   to read each `contribution.status`).
 * - {@link AnseoObserver.send} and the {@link observe} wrapper — **best-effort**.
 *   Observability must never interrupt the host app: any transport or server
 *   error is logged (DEBUG, or WARN for a `401`) and swallowed. **No status is
 *   ever retried** — at-most-once delivery is intentional (a retry on 5xx could
 *   double-record a run the server already processed before timing out).
 *
 * Zero runtime dependencies — uses the global `fetch` (Node >= 18).
 */

import { defaultLogger, type AnseoLogger } from "./logger";

const INGEST_PATH = "/v1/ingest/run";
const DEFAULT_TIMEOUT_MS = 30_000;

// Canonical auth + project headers (post-rename). The API also accepts the
// legacy `x-opengeo-*` spellings, but new clients send the canonical names.
const API_KEY_HEADER = "x-anseo-api-key";
const PROJECT_HEADER = "x-anseo-project";

/** Configuration for an {@link AnseoObserver}. */
export interface AnseoObserverConfig {
  /** Base URL of the Anseo API, e.g. `https://anseo.internal`. Trailing slash normalized. */
  baseUrl: string;
  /** API key, sent as the `x-anseo-api-key` header. Sole auth. */
  apiKey: string;
  /**
   * Project to scope the run to, sent as the `x-anseo-project` header (resolved
   * by brand name server-side). Optional for single-project deployments that
   * rely on the sole-active-project fallback.
   */
  project?: string;
  /** Request timeout in milliseconds. Defaults to 30000. */
  timeoutMs?: number;
  /**
   * Custom fetch implementation. Defaults to the global `fetch`. Useful for
   * tests (mock transport) or for injecting a proxy-wrapped fetch.
   */
  fetch?: typeof fetch;
  /** Custom diagnostics sink. Defaults to the console-backed `anseo` logger. */
  logger?: AnseoLogger;
}

/** One externally-executed run to record. Mirrors the API's `IngestRunRequest`. */
export interface ObserveRunInput {
  /** Declared prompt slug within the project. Must already exist server-side. */
  promptSlug: string;
  /** Provider that produced the run, e.g. `"openai"`; `"unknown"` if undetectable. */
  provider: string;
  /** Provider model version, e.g. `"gpt-4o-2024-08-06"`. */
  model: string;
  /** Raw response text the provider returned. Optional. */
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

/**
 * Raised at construction for an invalid SDK configuration (missing `baseUrl`
 * or `apiKey`). A misconfigured client is a programming error the developer
 * must fix; it is thrown eagerly at construction, never deferred to a call.
 * Mirrors the Python reference's `AnseoConfigError`.
 */
export class AnseoConfigError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "AnseoConfigError";
  }
}

/** Raised by the strict {@link AnseoObserver.observeRun} on a non-2xx response. */
export class AnseoApiError extends Error {
  /** HTTP status code. */
  readonly status: number;
  /** Machine-readable error code from the API body, if present. */
  readonly code: string | undefined;

  constructor(status: number, code: string | undefined, message: string) {
    super(message);
    this.name = "AnseoApiError";
    this.status = status;
    this.code = code;
  }
}

/** Backwards-compatible alias for the pre-40.3 name. */
export const OpenGeoApiError = AnseoApiError;

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
export class AnseoObserver {
  private readonly baseUrl: string;
  private readonly apiKey: string;
  private readonly project?: string;
  private readonly timeoutMs: number;
  private readonly fetchImpl: typeof fetch;
  /** Diagnostics sink; used by the best-effort {@link send} surface. */
  readonly logger: AnseoLogger;

  constructor(config: AnseoObserverConfig) {
    if (!config.baseUrl) {
      throw new AnseoConfigError("AnseoObserver: `baseUrl` is required");
    }
    if (!config.apiKey) {
      throw new AnseoConfigError("AnseoObserver: `apiKey` is required");
    }
    // Normalize trailing slash so URL joining is unambiguous.
    this.baseUrl = config.baseUrl.replace(/\/+$/, "");
    this.apiKey = config.apiKey;
    this.project = config.project;
    this.timeoutMs = config.timeoutMs ?? DEFAULT_TIMEOUT_MS;
    this.logger = config.logger ?? defaultLogger;
    const f = config.fetch ?? globalThis.fetch;
    if (typeof f !== "function") {
      throw new AnseoConfigError(
        "AnseoObserver: no `fetch` available — pass `fetch` in the config or run on a platform that provides a global fetch (Node >= 18).",
      );
    }
    this.fetchImpl = f;
  }

  /**
   * Strict send: record one externally-executed run, resolving with the parsed
   * {@link ObserveRunResult}; rejects with {@link AnseoApiError} on a non-2xx
   * response (and propagates transport/timeout errors).
   */
  async observeRun(input: ObserveRunInput): Promise<ObserveRunResult> {
    const headers: Record<string, string> = {
      "content-type": "application/json",
      [API_KEY_HEADER]: this.apiKey,
    };
    if (this.project) headers[PROJECT_HEADER] = this.project;

    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), this.timeoutMs);
    let response: Response;
    try {
      response = await this.fetchImpl(`${this.baseUrl}${INGEST_PATH}`, {
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
      throw new AnseoApiError(
        response.status,
        body?.error,
        body?.message ??
          `Anseo ingest failed: HTTP ${response.status}${text ? ` — ${text}` : ""}`,
      );
    }

    return parsed as ObserveRunResult;
  }

  /**
   * Best-effort, at-most-once send. Never rejects, never retries.
   *
   * Resolves with the {@link ObserveRunResult} on success, or `null` when the
   * run could not be delivered. Per the core spec, observability failures must
   * not interrupt the host app:
   *
   * - transport/timeout/decode errors are logged at DEBUG and discarded;
   * - a `401` (bad API key) is logged at WARN so the operator notices, but is
   *   still swallowed;
   * - **no** status is ever retried (at-most-once delivery).
   *
   * Enable diagnostics with `DEBUG=anseo`.
   */
  async send(input: ObserveRunInput): Promise<ObserveRunResult | null> {
    try {
      return await this.observeRun(input);
    } catch (err) {
      if (err instanceof AnseoApiError) {
        if (err.status === 401) {
          this.logger.warn(
            "ingest rejected (401) — check your API key; this run was NOT recorded: %s",
            err.message,
          );
        } else {
          this.logger.debug(
            "ingest returned HTTP %s (%s); run discarded: %s",
            err.status,
            err.code,
            err.message,
          );
        }
        return null;
      }
      // network/timeout/decode — best-effort: log and discard.
      this.logger.debug("ingest send failed; run discarded: %o", err);
      return null;
    }
  }
}

/**
 * One-shot strict convenience: construct an observer and record a single run.
 * Prefer reusing an {@link AnseoObserver} when sending many runs.
 */
export async function observeRun(
  config: AnseoObserverConfig,
  input: ObserveRunInput,
): Promise<ObserveRunResult> {
  return new AnseoObserver(config).observeRun(input);
}
