// Shared fetch-client internals for the OpenGEO API (apps/api). Server
// components call the per-feature fetchers (re-exported from `@/lib/api`)
// directly during render; the API runs at OGEO_API_BASE_URL (defaults to the
// compose service name `api` or http://127.0.0.1:8080 outside compose).

import { getSelectedProject } from "@/lib/projects";

export const API_BASE_URL =
  process.env.OGEO_API_BASE_URL ?? "http://127.0.0.1:8080";

/** Wire header carrying the per-request project (Epic 36; resolved by name). */
const PROJECT_HEADER = "X-OpenGEO-Project";

/**
 * Build the headers every API call shares: the server-only API key plus the
 * operator-selected project (Story 36.8). The project is read from the request
 * cookie so both SSR fetchers and the `app/api/*` proxy handlers forward the
 * exact same `X-OpenGEO-Project` value the switcher chose. When no project is
 * selected the header is omitted and the API falls back to the single active
 * project (ADR-004 tier 3).
 */
async function baseHeaders(json: boolean): Promise<Record<string, string>> {
  const headers: Record<string, string> = {};
  if (json) headers["Content-Type"] = "application/json";
  // Read the operator-provided key from env at request time so a hot dashboard
  // reload picks up rotations without a restart.
  const apiKey = process.env.OGEO_API_KEY;
  if (apiKey) headers["X-OpenGEO-API-Key"] = apiKey;
  const project = await getSelectedProject();
  if (project) headers[PROJECT_HEADER] = project;
  return headers;
}

export async function getJson<T>(path: string): Promise<T> {
  const url = `${API_BASE_URL}${path}`;
  // Phase 2: the /v1 routes require X-OpenGEO-API-Key. The legacy /api root
  // paths share the same middleware now (Story 12.1 decision 3).
  const headers = await baseHeaders(false);
  // Disable Next.js fetch caching so the dashboard always shows fresh data.
  const r = await fetch(url, { cache: "no-store", headers });
  if (!r.ok) {
    throw new Error(`GET ${url} -> ${r.status} ${r.statusText}`);
  }
  return (await r.json()) as T;
}

export async function setupHeaders(json: boolean): Promise<Record<string, string>> {
  return baseHeaders(json);
}
