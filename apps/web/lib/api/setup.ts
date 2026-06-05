// Setup/status surface: Story 15.2 status, 15.3 ClickHouse install SSE,
// 15.4 remote-connect, 15.5 ETL status, 15.6 webhook test + api-key revoke.

import { API_BASE_URL, getJson, setupHeaders } from "./_client";

// ─── DB-authoritative brand config (live) ───────────────────────────────────

export interface CompetitorConfig {
  name: string;
  variants: string[];
}

export interface BrandView {
  project_id: string;
  name: string;
  variants: string[];
  competitors: CompetitorConfig[];
  /** Optional owned-website URL. Scopes /audit and crawler observability. */
  site_url?: string;
}

export interface BrandUpdate {
  name: string;
  variants: string[];
  competitors: CompetitorConfig[];
  site_url?: string;
}

export interface BrandUpdateResult extends BrandView {
  /** True when the name changed and project_id was re-derived; the operator
   *  must restart the API for the new identity to take effect. */
  restart_required: boolean;
  error?: string;
  message?: string;
}

/** GET the DB-authoritative brand config. Called from client components, so it
 *  targets the same-origin Next route which attaches the operator key. */
export async function getBrand(): Promise<BrandView> {
  const r = await fetch(`/api/setup/brand`, { method: "GET", cache: "no-store" });
  if (!r.ok) {
    throw new Error(`GET /api/setup/brand -> ${r.status} ${r.statusText}`);
  }
  return (await r.json()) as BrandView;
}

/** Server-component variant: hits the API directly (absolute base URL +
 *  server-env key) so server pages can read the brand config. */
export async function fetchBrandConfig(): Promise<BrandView> {
  return getJson<BrandView>(`/v1/setup/brand`);
}

/** PUT the brand config. Editing only variants/competitors is an in-place
 *  update; changing the name re-derives project_id (allowed only before the
 *  first run) and sets `restart_required`. A rename blocked by existing runs
 *  surfaces as a 409 with `error: "rename_blocked_has_runs"`. */
export async function putBrand(body: BrandUpdate): Promise<BrandUpdateResult> {
  const r = await fetch(`/api/setup/brand`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
    cache: "no-store",
  });
  const parsed = (await r.json()) as BrandUpdateResult;
  if (!r.ok && !parsed.error) {
    parsed.error = `PUT /api/setup/brand -> ${r.status} ${r.statusText}`;
  }
  return parsed;
}

// ─── AI competitor suggestion (Epic 35) ─────────────────────────────────────

export interface SuggestCompetitorsResult {
  competitors: CompetitorConfig[];
  provider: string;
  model: string;
  error?: string;
  message?: string;
}

/** Ask a configured provider to suggest competitors for the current brand.
 *  `provider` is a wire name (openai, anthropic, …) that must have a stored
 *  key. Nothing is persisted — the editor merges + saves via putBrand. Targets
 *  the same-origin Next route which attaches the operator key. */
export async function suggestCompetitors(
  provider: string,
): Promise<SuggestCompetitorsResult> {
  const r = await fetch(`/api/setup/brand/suggest-competitors`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ provider }),
    cache: "no-store",
  });
  const parsed = (await r.json()) as SuggestCompetitorsResult;
  if (!r.ok && !parsed.error) {
    parsed.error = `POST /api/setup/brand/suggest-competitors -> ${r.status} ${r.statusText}`;
  }
  return parsed;
}

// ─── Story 15.2 setup/status endpoint (live) ─────────────────────────────────

export interface SetupStatus {
  postgres: {
    state: "healthy" | "degraded" | "unknown";
    schema_version: number | null;
    row_count_estimate: number | null;
    last_write_at: string | null;
    error?: string;
  };
  clickhouse: {
    state: "healthy" | "degraded" | "not_configured" | "unknown";
    url: string | null;
    row_count: number | null;
    etl_lag_seconds: number | null;
    error?: string;
  };
  worker: {
    state: "running" | "stopped" | "unknown";
    uptime_seconds: number | null;
    queue_depth: number | null;
    error?: string;
  };
  webhook_target: {
    configured: boolean;
    last_delivery_at: string | null;
    last_status: string | null;
    error?: string;
  };
  api_keys: Array<{
    provider: string;
    configured: boolean;
    last_used_at: string | null;
  }>;
  docker: {
    present: boolean;
    version: string | null;
    error?: string;
  };
}

export async function fetchSetupStatus(
  opts: { mockEmpty?: boolean } = {},
): Promise<SetupStatus> {
  // `mockEmpty` forwards `?empty=1` to the E2E mock backend so the Playwright
  // empty-state spec can drive an empty `api_keys` / unconfigured webhook
  // through SSR (page.route() can't intercept server-side fetches). The real
  // API ignores the unknown query param, so this is inert in production.
  const path = opts.mockEmpty ? "/v1/setup/status?empty=1" : "/v1/setup/status";
  return getJson<SetupStatus>(path);
}

// ─── Story 15.3 ClickHouse local install (live SSE) ──────────────────────────

/** Canonical install step sequence (apps/api setup.rs `INSTALL_STEPS`).
 *  These strings are the wire contract; the UI maps them to operator copy. */
export type InstallStep =
  | "docker_detected"
  | "image_pulling"
  | "container_starting"
  | "provisioning_user"
  | "applying_migrations"
  | "running_parity_test"
  | "complete";

export interface InstallEvent {
  step: InstallStep | string;
  /** 0..1 fraction of completed steps. */
  progress: number;
  log_line: string;
  at: string;
}

export interface InstallAccepted {
  install_id: string;
  /** Relative SSE path, e.g. `/v1/setup/clickhouse/install-stream?id=<ulid>`. */
  stream: string;
}

/** POST /v1/setup/clickhouse/install — kicks off the install state machine.
 *  Returns the install id + the SSE stream path to consume for progress. */
export async function postClickHouseInstall(): Promise<InstallAccepted> {
  const r = await fetch(`${API_BASE_URL}/v1/setup/clickhouse/install`, {
    method: "POST",
    headers: await setupHeaders(true),
    cache: "no-store",
  });
  if (!r.ok) {
    throw new Error(
      `POST /v1/setup/clickhouse/install -> ${r.status} ${r.statusText}`,
    );
  }
  return (await r.json()) as InstallAccepted;
}

/** Consume the install SSE stream, invoking `onEvent` for each `install`
 *  event until the stream closes. Uses fetch + a streaming reader (rather
 *  than EventSource) so the X-OpenGEO-API-Key header can be attached.
 *  `streamPath` is the relative path returned by {@link postClickHouseInstall}. */
export async function streamClickHouseInstall(
  streamPath: string,
  onEvent: (event: InstallEvent) => void,
  signal?: AbortSignal,
): Promise<void> {
  const r = await fetch(`${API_BASE_URL}${streamPath}`, {
    method: "GET",
    headers: await setupHeaders(false),
    cache: "no-store",
    signal,
  });
  if (!r.ok || !r.body) {
    throw new Error(
      `GET ${streamPath} -> ${r.status} ${r.statusText}`,
    );
  }
  const reader = r.body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    buffer += decoder.decode(value, { stream: true });
    // SSE frames are separated by a blank line.
    let sep: number;
    while ((sep = buffer.indexOf("\n\n")) !== -1) {
      const frame = buffer.slice(0, sep);
      buffer = buffer.slice(sep + 2);
      const dataLine = frame
        .split("\n")
        .find((l) => l.startsWith("data:"));
      if (!dataLine) continue;
      const json = dataLine.slice("data:".length).trim();
      if (!json) continue;
      try {
        onEvent(JSON.parse(json) as InstallEvent);
      } catch {
        // Skip non-JSON frames (e.g. keep-alive `ping`).
      }
    }
  }
}

// ─── Story 15.4 ClickHouse remote-connect (live) ─────────────────────────────

export type ConnectPreset =
  | "tinybird"
  | "aiven"
  | "clickhouse_cloud"
  | "custom";

export interface ConnectRequest {
  preset: ConnectPreset;
  endpoint: string;
  username?: string;
  password?: string;
  database?: string;
}

export type ConnectState =
  | "connected"
  | "invalid_credentials"
  | "unreachable"
  | "schema_incompatible"
  | "bad_request"
  | "persist_failed";

export interface ConnectResult {
  ok: boolean;
  state: ConnectState;
  message: string;
  endpoint?: string;
}

/** POST /v1/setup/clickhouse/connect — probes the remote ClickHouse and, on
 *  success, persists the endpoint to opengeo.yaml. Returns a structured
 *  result; the password is sent for the probe only and never stored. */
export async function postClickHouseConnect(
  req: ConnectRequest,
): Promise<ConnectResult> {
  const r = await fetch(`${API_BASE_URL}/v1/setup/clickhouse/connect`, {
    method: "POST",
    headers: await setupHeaders(true),
    body: JSON.stringify(req),
    cache: "no-store",
  });
  // The endpoint returns structured failures with 200 OR a 400 for malformed
  // input; parse the body in both cases so the UI can render the banner.
  try {
    return (await r.json()) as ConnectResult;
  } catch {
    return {
      ok: false,
      state: "unreachable",
      message: `connect request failed: ${r.status} ${r.statusText}`,
    };
  }
}

// ─── Story 30-8 ETL status + resume (live) ───────────────────────────────────

/** Wire shape of the backend `EtlStatus` struct (apps/api setup.rs, Story
 *  30-8b). Field names/nullability mirror the Rust `#[derive(Serialize)]`:
 *  the numeric checkpoint fields are `Option<i64>` → `number | null`, and the
 *  timestamps serialize as RFC-3339 strings via `Option<String>`. */
export interface ClickHouseEtlStatus {
  state: "idle" | "running" | "interrupted" | "completed" | "unknown";
  batches_done: number | null;
  batches_total: number | null;
  last_heartbeat_at: string | null; // RFC-3339 timestamp or null
  finished_at: string | null; // RFC-3339 timestamp or null
  error: string | null; // populated only on the "unknown" state
}

/** Response of POST /v1/setup/clickhouse/resume (202). `triggered` is false
 *  when there was no prior checkpoint to re-arm (operator must run a fresh
 *  migration). */
export interface ClickHouseEtlResumeResult {
  triggered: boolean;
  message: string;
}

/** GET /v1/setup/clickhouse/status — reads the resumable-ETL checkpoint row
 *  (`analytics_migration_state`) and derives a migration state. Always 200;
 *  the backend reports `state: "unknown"` with an `error` message rather than
 *  failing when the underlying query errors. */
export async function fetchClickHouseEtlStatus(): Promise<ClickHouseEtlStatus> {
  return getJson<ClickHouseEtlStatus>("/v1/setup/clickhouse/status");
}

/** POST /v1/setup/clickhouse/resume — re-arms the ETL checkpoint so the
 *  out-of-process migrator resumes from the last completed batch. Returns 202
 *  with `{ triggered, message }`. */
export async function postClickHouseEtlResume(): Promise<ClickHouseEtlResumeResult> {
  const r = await fetch(`${API_BASE_URL}/v1/setup/clickhouse/resume`, {
    method: "POST",
    headers: await setupHeaders(false),
    cache: "no-store",
  });
  if (!r.ok) {
    throw new Error(
      `POST /v1/setup/clickhouse/resume -> ${r.status} ${r.statusText}`,
    );
  }
  return (await r.json()) as ClickHouseEtlResumeResult;
}

// ─── Story 15.6 webhook test + api-key revoke (mocked — endpoints pending) ────

export interface WebhookTestResult {
  status_code: number | null;
  signature_valid: boolean | null;
  latency_ms: number | null;
  error: string | null;
}

/** POST /v1/setup/webhook/test — fires a signed test webhook at the given URL.
 *  Returns signature verification status + response code.
 *  Endpoint is a stub in Story 15.1 (returns a placeholder); real flow lands in Story 15.3. */
export async function postWebhookTest(url: string): Promise<WebhookTestResult> {
  // Real endpoint: POST /v1/setup/webhook/test with { url }
  // For now return mock success since backend stub may not handle all cases
  try {
    const r = await fetch(`${API_BASE_URL}/v1/setup/webhook/test`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        ...(process.env.ANSEO_API_KEY ? { "X-OpenGEO-API-Key": process.env.ANSEO_API_KEY } : {}),
      },
      body: JSON.stringify({ url }),
      cache: "no-store",
    });
    if (!r.ok) {
      return { status_code: r.status, signature_valid: null, latency_ms: null, error: r.statusText };
    }
    return (await r.json()) as WebhookTestResult;
  } catch (err) {
    return { status_code: null, signature_valid: null, latency_ms: null, error: String(err) };
  }
}

export interface ApiKeySetResult {
  configured: boolean;
  provider?: string;
  message?: string;
  error?: string;
}

/** Store a provider API key. Invoked from client components, so it targets
 *  the same-origin Next route handler (app/api/setup/api-keys/[provider]),
 *  which proxies to the backend server-side with the operator key attached.
 *  The key is sent for persistence only and is never echoed back. */
export async function postApiKeySet(
  provider: string,
  key: string,
): Promise<ApiKeySetResult> {
  const r = await fetch(
    `/api/setup/api-keys/${encodeURIComponent(provider)}`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ key }),
      cache: "no-store",
    },
  );
  try {
    return (await r.json()) as ApiKeySetResult;
  } catch {
    return {
      configured: false,
      error: `set key failed: ${r.status} ${r.statusText}`,
    };
  }
}

/** Revoke a provider API key. Called from client components, so it targets the
 *  same-origin Next route handler, which proxies to the backend server-side. */
export async function postApiKeyRevoke(provider: string): Promise<void> {
  try {
    await fetch(`/api/setup/api-keys/${encodeURIComponent(provider)}/revoke`, {
      method: "POST",
      cache: "no-store",
    });
  } catch {
    // Best-effort — swallow errors; the UI updates optimistically.
  }
}
