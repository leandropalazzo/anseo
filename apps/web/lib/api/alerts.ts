// Alerts feature (Story 31-2).
//
// Live fetchers + mutation for the alert-rules surface. Backed by the
// alert-rules endpoints from story 31-1:
//   GET   /v1/alert-rules           → { items: AlertRule[] }
//   PATCH /v1/alert-rules/:name     body { status: "armed" | "muted" }
//
// The anomaly inbox is sourced separately via the already-live
// `fetchAnomalies` (lib/api/anomalies.ts). This module owns only the rules.

import { API_BASE_URL, getJson, setupHeaders } from "./_client";

/** Wire-stable rule status (apps/api alert-rules). */
export type AlertRuleStatus = "armed" | "muted";

/** Single alert rule, matching the backend `/v1/alert-rules` item shape. */
export interface AlertRule {
  name: string;
  /** Human-readable condition expression. */
  on: string;
  /** Prompt name or `*` for all. */
  target: string;
  channels: string[];
  status: AlertRuleStatus;
  /** Number of times the rule fired in the last 7 days (derived). */
  fires: number;
}

/** Backend `GET /v1/alert-rules` envelope. */
interface AlertRulesResponse {
  items: AlertRule[];
}

/** Fetch the configured alert rules with their last-7d fire counts. */
export async function fetchAlertRules(): Promise<AlertRule[]> {
  const { items } = await getJson<AlertRulesResponse>(`/v1/alert-rules`);
  return items;
}

/**
 * Arm or mute an alert rule.
 *
 * `PATCH /v1/alert-rules/:name` with body `{ status }`. Returns the updated
 * rule. Callers should `router.refresh()` after a successful toggle so the
 * server-rendered rules table re-fetches.
 */
export async function setAlertRuleStatus(
  name: string,
  status: AlertRuleStatus,
): Promise<AlertRule> {
  const r = await fetch(
    `${API_BASE_URL}/v1/alert-rules/${encodeURIComponent(name)}`,
    {
      method: "PATCH",
      headers: await setupHeaders(true),
      body: JSON.stringify({ status }),
      cache: "no-store",
    },
  );
  if (!r.ok) {
    throw new Error(`PATCH alert-rule -> ${r.status} ${r.statusText}`);
  }
  return (await r.json()) as AlertRule;
}
