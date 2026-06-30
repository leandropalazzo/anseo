// Shared fetch-client internals for the OpenGEO API (apps/api). Server
// components call the per-feature fetchers (re-exported from `@/lib/api`)
// directly during render; the API runs at ANSEO_API_BASE_URL (defaults to the
// compose service name `api` or http://127.0.0.1:8080 outside compose).

import { getSelectedProject } from "@/lib/projects";

export const API_BASE_URL =
  process.env.ANSEO_API_BASE_URL ?? "http://127.0.0.1:8080";

/** Wire header carrying the per-request project (Epic 36; resolved by name). */
const PROJECT_HEADER = "X-Anseo-Project";

/** Short-lived server-side cache of valid project names to avoid a /v1/projects
 *  round-trip on every request. Invalidated after 30 s. */
let projectCache: { names: Set<string>; first: string; ts: number } | null = null;

async function resolveValidProject(
  cookie: string | undefined,
  apiKey: string,
): Promise<string | undefined> {
  const now = Date.now();
  // Refresh the cache when absent or older than 30 s.
  if (!projectCache || now - projectCache.ts > 30_000) {
    try {
      const r = await fetch(`${API_BASE_URL}/v1/projects`, {
        headers: { "X-Anseo-API-Key": apiKey },
        cache: "no-store",
      });
      if (r.ok) {
        const data = (await r.json()) as { projects?: Array<{ name: string }> };
        const list = data.projects ?? [];
        if (list.length > 0) {
          projectCache = {
            names: new Set(list.map((p) => p.name)),
            first: list[0].name,
            ts: now,
          };
        }
      }
    } catch {
      // API unreachable — fall through and omit the header.
    }
  }
  if (!projectCache) return undefined;
  // Use the cookie value when it names a real project; otherwise fall back to
  // the first project so a stale cookie never causes a 404.
  return cookie && projectCache.names.has(cookie) ? cookie : projectCache.first;
}

/**
 * Build the headers every API call shares: the server-only API key plus the
 * operator-selected project (Story 36.8). Validates the cookie value against
 * the live project list so a stale cookie never sends an unknown project name.
 */
async function baseHeaders(json: boolean): Promise<Record<string, string>> {
  const headers: Record<string, string> = {};
  if (json) headers["Content-Type"] = "application/json";
  const apiKey = process.env.ANSEO_API_KEY;
  if (apiKey) headers["X-Anseo-API-Key"] = apiKey;
  const cookie = await getSelectedProject();
  if (apiKey) {
    const project = await resolveValidProject(cookie, apiKey);
    if (project) headers[PROJECT_HEADER] = project;
  } else if (cookie) {
    // No API key configured (dev without auth) — trust the cookie as-is.
    headers[PROJECT_HEADER] = cookie;
  }
  return headers;
}

export async function getJson<T>(path: string): Promise<T> {
  const url = `${API_BASE_URL}${path}`;
  // Phase 2: the /v1 routes require X-Anseo-API-Key. The legacy /api root
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
