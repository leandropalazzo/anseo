// Epic 34 Story 3 — hallucination / brand-accuracy surface (live).
// Wires to GET /v1/hallucination/summary. Judgment is premium-gated
// (open-core boundary, story 34-4): the OSS build collects claims and
// ground-truth facts but returns `premium_disabled` verdicts.

import { getJson } from "./_client";

export type Entitlement = "oss_disabled" | "premium_enabled";
export type AccuracyStatus =
  | "accurate"
  | "inaccurate"
  | "unverifiable"
  | "premium_disabled";

export interface HallucinationTotals {
  accurate: number;
  inaccurate: number;
  unverifiable: number;
  premium_disabled: number;
  total: number;
}

export interface ClaimVerdict {
  entity: string;
  claim_text: string;
  claim_kind: string;
  status: AccuracyStatus;
  rationale: string;
  matched_fact_key: string | null;
  prompt_run_id: string;
  observed_at: string;
}

export interface HallucinationSummary {
  entitlement: Entitlement;
  window_days: number;
  ground_truth_facts: number;
  totals: HallucinationTotals;
  /** Newest-first claims; inaccurate claims are the alert surface. */
  recent: ClaimVerdict[];
}

export async function fetchHallucinationSummary(
  days = 30,
): Promise<HallucinationSummary> {
  const qs = new URLSearchParams({ days: String(days) });
  return getJson(`/v1/hallucination/summary?${qs.toString()}`);
}
