// Epic 32 Story 5 — site-audit surface (live).
// Wires to POST /v1/audit, which runs the in-tree citation-readiness engine
// (crates/audit) against a URL/sitemap and returns a scored report.

import { API_BASE_URL, getJson } from "./_client";

export type AuditCategory = "identity" | "extractability" | "corroboration";
export type AuditSeverity = "low" | "medium" | "high";
export type FindingStatus = "pass" | "warn" | "fail";

export interface AuditFinding {
  rule_id: string;
  category: AuditCategory;
  severity: AuditSeverity;
  status: FindingStatus;
  score: number;
  message: string;
  recommendation_kind: string;
  evidence: string[];
}

export interface PageAudit {
  url: string;
  title: string | null;
  score: number;
  findings: AuditFinding[];
}

export interface GateFinding {
  page_url: string;
  rule_id: string;
  severity: AuditSeverity;
  message: string;
}

export interface GateSummary {
  passed: boolean;
  fail_on: string[];
  failed_findings: GateFinding[];
}

export interface AuditReport {
  target: string;
  overall_score: number;
  pages: PageAudit[];
  gate: GateSummary | null;
}

export interface AuditRequest {
  target: string;
  max_pages?: number;
  timeout_ms?: number;
  fail_on?: string[];
}

export interface AuditRunItem {
  id: string;
  target: string;
  overall_score: number;
  pages_crawled: number;
  gate_passed: boolean | null;
  created_at: string;
}

/** Persisted audit history for the project (server-component fetch). */
export async function fetchAuditRuns(limit = 20): Promise<{ items: AuditRunItem[] }> {
  return getJson(`/v1/audit/runs?limit=${limit}`);
}

/** Run an audit. POST (not getJson) because it triggers a live crawl. */
export async function runAudit(req: AuditRequest): Promise<AuditReport> {
  const headers: Record<string, string> = { "Content-Type": "application/json" };
  const apiKey = process.env.ANSEO_API_KEY;
  if (apiKey) headers["X-Anseo-API-Key"] = apiKey;
  const r = await fetch(`${API_BASE_URL}/v1/audit`, {
    method: "POST",
    cache: "no-store",
    headers,
    body: JSON.stringify(req),
  });
  if (!r.ok) {
    throw new Error(`POST /v1/audit -> ${r.status} ${r.statusText}`);
  }
  return (await r.json()) as AuditReport;
}
