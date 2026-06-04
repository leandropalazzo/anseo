// Anomalies feature.
//
// `AnomalyItem` mirrors the backend `AnomaliesResponse` item shape
// (apps/api/src/routes/anomalies.rs `AnomalyItem`). This type — and the
// `AnomalyTimelineProps` contract below — are the frozen wire/prop surface
// that downstream stories (30-3 overview, 30-4) build against; treat both as
// stable.

import { getJson } from "./_client";

/** Wire-stable anomaly taxonomy (apps/api `AnomalyItemKind`). */
export type AnomalyKind = "visibility_drop" | "citation_loss" | "rank_swap";

/** Wire-stable severity (apps/api `AnomalySeverity`). */
export type AnomalySeverity = "low" | "medium" | "high";

/** Single anomaly row, matching apps/api `AnomaliesResponse` item shape. */
export interface AnomalyItem {
  /** ULID-form ID stable per emission. */
  id: string;
  kind: AnomalyKind;
  /** Prompt slug, best-effort; absent for project-wide anomalies. */
  prompt?: string;
  /** Wire-stable provider name (`openai|anthropic|gemini|perplexity|…`). */
  provider: string;
  detected_at: string;
  severity: AnomalySeverity;
  /** Opaque effect-size signal; treat as opaque for ranking. */
  delta: number;
  window_days: number;
  /** Verbatim detector detail blob. */
  details: unknown;
}

/** Frozen prop contract for the Overview AnomalyTimeline component.
 *  Downstream stories (30-3/30-4) depend on this exact shape. */
export interface AnomalyTimelineProps {
  items: AnomalyItem[];
}

/** Detection window for the anomaly feed. */
export type AnomalyWindow = "1d" | "7d" | "30d";

/** Backend `GET /anomalies` envelope. */
interface AnomaliesResponse {
  items: AnomalyItem[];
  trace_id: string;
}

/**
 * Anomaly feed for the Overview AnomalyTimeline.
 *
 * `GET /anomalies?window=<window>` (apps/api `anomalies.rs`). Returns the
 * `items` array straight from the envelope; the caller passes it through the
 * frozen `AnomalyTimelineProps` contract above.
 */
export async function fetchAnomalies(
  window: AnomalyWindow = "7d",
): Promise<AnomalyItem[]> {
  const { items } = await getJson<AnomaliesResponse>(
    `/anomalies?window=${window}`,
  );
  return items;
}
