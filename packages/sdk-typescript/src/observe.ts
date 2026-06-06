/**
 * The `observe` instrumentation surface (Story 40.3).
 *
 * Wraps an existing LLM call so its run ships to Anseo without changing your
 * inference logic. This is the TypeScript port of the Python reference's
 * `observe` context-manager/decorator. JavaScript has no decorator-or-context
 * duality, so the same capability is exposed two idiomatic ways from one spec:
 *
 * - **{@link observe}** — pass the call as a function; its return value is
 *   treated as the raw response and captured + shipped automatically::
 *
 *       const resp = await observe(
 *         observer,
 *         { promptSlug: "best-sunglasses" },
 *         () => client.chat.completions.create(...),
 *       );
 *
 * - **{@link startRun}** — for manual control: run the call yourself, then
 *   `run.capture(resp)` (auto-detects provider/model + extracts text) and
 *   `await run.ship()`::
 *
 *       const run = startRun(observer, { promptSlug: "best-sunglasses" });
 *       const resp = await client.chat.completions.create(...);
 *       run.capture(resp);
 *       await run.ship();
 *
 * Delivery is best-effort and at-most-once (see {@link AnseoObserver.send}):
 * observability never throws into, or retries inside, the host app. If the
 * wrapped call itself throws, **nothing is sent** (there is no run to record)
 * and the error propagates unchanged.
 *
 * This module never patches the OpenAI/Anthropic SDKs; it only reads documented
 * attributes off the response object you hand it.
 */

import type { AnseoObserver, ObserveRunResult } from "./client";
import { detectProviderModel, extractText } from "./detect";

/** Options shared by {@link observe} and {@link startRun}. */
export interface ObserveOptions {
  /** The declared prompt slug for this run. */
  promptSlug: string;
  /** Explicit provider; overrides auto-detection. */
  provider?: string;
  /** Explicit model; overrides auto-detection. */
  model?: string;
  /** Pre-computed brand rank for the run. */
  observedRank?: number;
  /** Pre-extracted citation domains. */
  citationDomains?: string[];
}

/**
 * A handle for manual instrumentation. Collect the call's output via
 * {@link capture} (auto-detect) or by setting the explicit fields, then
 * {@link ship} the run best-effort.
 */
export class ObserveRunHandle {
  private provider: string | undefined;
  private model: string | undefined;
  private responseText: string | undefined;
  private readonly observedRank: number | undefined;
  private readonly citationDomains: string[] | undefined;
  private captured = false;

  constructor(
    private readonly observer: AnseoObserver,
    private readonly options: ObserveOptions,
  ) {
    this.provider = options.provider;
    this.model = options.model;
    this.observedRank = options.observedRank;
    this.citationDomains = options.citationDomains;
  }

  /**
   * Auto-detect provider/model and extract text from `rawResponse`. Explicit
   * `provider`/`model` override auto-detection. Calling this marks the run for
   * delivery.
   */
  capture(
    rawResponse: unknown,
    overrides: { provider?: string; model?: string } = {},
  ): this {
    const det = detectProviderModel(rawResponse);
    this.provider = overrides.provider ?? this.provider ?? det.provider;
    this.model = overrides.model ?? this.model ?? det.model;
    const text = extractText(rawResponse);
    if (text !== undefined) this.responseText = text;
    this.captured = true;
    return this;
  }

  /**
   * Ship the captured run, best-effort. Resolves with the
   * {@link ObserveRunResult} on success, or `null` when nothing was sent (no
   * `capture()`, an undetermined model, or a swallowed delivery failure).
   */
  async ship(): Promise<ObserveRunResult | null> {
    if (!this.captured) {
      this.observer.logger.debug(
        "observe() run for %s exited without capture(); nothing sent",
        this.options.promptSlug,
      );
      return null;
    }
    // provider falls back to the server-validated sentinel "unknown".
    const provider = this.provider || "unknown";
    if (!this.model) {
      this.observer.logger.debug(
        "could not determine model for %s; skipping send (supply model explicitly)",
        this.options.promptSlug,
      );
      return null;
    }
    return this.observer.send({
      promptSlug: this.options.promptSlug,
      provider,
      model: this.model,
      responseText: this.responseText,
      citationDomains: this.citationDomains,
      observedRank: this.observedRank,
      observedAt: new Date(),
    });
  }
}

/**
 * Start a manual instrumentation run. The caller runs the LLM call, then
 * `run.capture(resp)` and `await run.ship()`.
 */
export function startRun(
  observer: AnseoObserver,
  options: ObserveOptions,
): ObserveRunHandle {
  return new ObserveRunHandle(observer, options);
}

/**
 * Instrument an LLM call. Runs `fn`, treats its resolved value as the raw
 * response, captures (auto-detecting provider/model + text), and ships the run
 * best-effort. Returns `fn`'s value unchanged.
 *
 * If `fn` throws/rejects, nothing is sent and the error propagates unchanged.
 */
export async function observe<T>(
  observer: AnseoObserver,
  options: ObserveOptions,
  fn: () => T | Promise<T>,
): Promise<T> {
  const result = await fn(); // throws => nothing sent
  const run = new ObserveRunHandle(observer, options);
  run.capture(result);
  await run.ship();
  return result;
}
