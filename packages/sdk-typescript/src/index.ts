/**
 * `@anseo/observe` — thin instrumentation SDK for the Anseo Run-Ingestion API.
 *
 * The OpenTelemetry pattern, minus the ceremony: you already ran a prompt
 * against an LLM provider *outside* Anseo. This SDK lets you POST that run to
 * `POST /v1/ingest/run` in one call, so it flows through the same
 * extraction → redaction → benchmark-contribution path as a native run.
 *
 * TypeScript port of the Python reference (`packages/sdk-python/anseo_observe`),
 * implementing the same language-agnostic spec in `docs/sdk-spec.md`. Zero
 * runtime dependencies — uses the global `fetch` (Node >= 18).
 *
 * ```ts
 * import { AnseoObserver, observe } from "@anseo/observe";
 *
 * const observer = new AnseoObserver({
 *   baseUrl: "https://anseo.internal",
 *   apiKey: process.env.ANSEO_API_KEY!,
 *   project: "Sunski",
 * });
 *
 * // Wrap the call — provider/model auto-detected, run shipped best-effort:
 * const resp = await observe(
 *   observer,
 *   { promptSlug: "best-polarized-sunglasses" },
 *   () => client.chat.completions.create({ ... }),
 * );
 *
 * // Or strict, for backfills that read each contribution status:
 * const result = await observer.observeRun({
 *   promptSlug: "best-polarized-sunglasses",
 *   provider: "openai",
 *   model: "gpt-4o-2024-08-06",
 *   responseText: resp.choices[0].message.content ?? "",
 * });
 * ```
 */

export {
  AnseoObserver,
  AnseoApiError,
  AnseoConfigError,
  OpenGeoApiError,
  observeRun,
  type AnseoObserverConfig,
  type ObserveRunInput,
  type ObserveRunResult,
  type ContributionStatus,
} from "./client.js";

export {
  observe,
  startRun,
  ObserveRunHandle,
  type ObserveOptions,
} from "./observe.js";

export { detectProviderModel, extractText } from "./detect.js";

export { defaultLogger, type AnseoLogger } from "./logger.js";
