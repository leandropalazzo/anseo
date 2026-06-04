// Run-detail feature (Story 30-7): live per-run extraction read API.
//
// Hits the Story 30-6 endpoints (see `apps/api/src/routes/run_detail.rs`):
//   GET /api/runs/:id/mentions    -> MentionEntry[]
//   GET /api/runs/:id/citations   -> CitationEntry[]
//   GET /api/runs/:id/provenance  -> ProvenanceStep[]  (always [] — no model yet)
//   GET /api/runs/:id/responses   -> ResponseEntry[]   (one entry per run)
//
// An unknown run id returns 404; a known run with no rows returns []. The TS
// types below mirror the `#[derive(Serialize)]` structs in run_detail.rs 1:1.

import { getJson } from "./_client";

/** One extracted mention. `provider` is denormalised from the parent run. */
export interface RunMentionEntry {
  id: string;
  entity: string;
  provider: string;
  /** Ranking / position of the mention in the response (the `rank` column). */
  rank: number;
  /** Character offset of the match in the raw response text. */
  char_offset: number;
  matched_text: string;
}

/** One citation. `provider` is denormalised from the parent run. */
export interface RunCitationEntry {
  id: string;
  domain: string;
  url: string | null;
  source_type: string | null;
  frequency: number;
  provider: string;
}

/**
 * A provenance / lifecycle step. The Phase 1 schema has no provenance model,
 * so this endpoint always returns []. The shape is forward-compatible.
 */
export interface RunProvenanceStep {
  step: string;
  status: string;
  /** ISO-8601 timestamp. */
  at: string;
}

/** The raw response captured for one (run, provider) pair. */
export interface RunResponseEntry {
  provider: string;
  provider_model_version: string;
  status: string;
  raw_response: unknown;
}

export async function fetchRunMentions(id: string): Promise<RunMentionEntry[]> {
  return getJson(`/api/runs/${encodeURIComponent(id)}/mentions`);
}

export async function fetchRunCitations(
  id: string,
): Promise<RunCitationEntry[]> {
  return getJson(`/api/runs/${encodeURIComponent(id)}/citations`);
}

export async function fetchRunProvenance(
  id: string,
): Promise<RunProvenanceStep[]> {
  return getJson(`/api/runs/${encodeURIComponent(id)}/provenance`);
}

export async function fetchRunResponses(
  id: string,
): Promise<RunResponseEntry[]> {
  return getJson(`/api/runs/${encodeURIComponent(id)}/responses`);
}
