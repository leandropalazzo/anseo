// Story 19.9 / UX-DR126 — a single recommendation envelope must render
// identically across surfaces. Both the Overview "Top Recommendations" tile
// and the /recommendations list build their priority + NDP markup from the
// same shared components; this test pins that they emit byte-identical marker
// HTML for the same envelope. The Rust contract test pins the wire side.

import { describe, it, expect, vi } from "vitest";
import { render } from "@testing-library/react";

import { TopRecommendations } from "@/app/_components/top-recommendations";
import {
  NdpMarkerFor,
  PriorityLabel,
} from "@/app/recommendations/_components/priority-label";
import type { Recommendation } from "@/lib/api";

vi.mock("next/link", () => ({
  default: ({
    href,
    children,
    ...rest
  }: {
    href: string;
    children: React.ReactNode;
  }) => (
    <a href={href} {...rest}>
      {children}
    </a>
  ),
}));

function rec(): Recommendation {
  return {
    id: "01JABCDEF0123456789ABCDEFG",
    project_id: "01PROJECT0000000000000000",
    kind: "visibility_gap",
    severity: "high",
    confidence_band: "medium",
    state: "surfaced",
    summary: "Brand visibility dropped on Perplexity",
    payload: {},
    traceability: {
      source_run_ids: [],
      source_run_ids_truncated: false,
      source_citation_ids: [],
      source_citation_ids_truncated: false,
      source_benchmark_queries: [],
      window: { start: "", end: "" },
      input_fingerprint: "",
    },
    reproducibility: { class: "non_deterministic", note: null },
    tags: ["non_deterministic_pipeline"],
    generated_at: "2026-05-30T00:00:00Z",
    engine_version: "sm14-1.0.0",
  };
}

function markerHtml(container: HTMLElement): {
  priority: string;
  ndp: string;
} {
  const priority = container.querySelector('[data-testid="rec-priority"]');
  const ndp = container.querySelector('[data-testid="rec-ndp-marker"]');
  return {
    priority: priority?.outerHTML ?? "",
    ndp: ndp?.outerHTML ?? "",
  };
}

describe("cross-surface recommendation rendering (UX-DR126)", () => {
  it("renders identical priority + NDP markers in the tile and the list", () => {
    const r = rec();

    // Surface A: the Overview tile.
    const tile = render(<TopRecommendations items={[r]} />);
    const tileMarkers = markerHtml(tile.container);

    // Surface B: a list row's marker cluster (same shared components).
    const list = render(
      <div>
        <PriorityLabel severity={r.severity} />
        <NdpMarkerFor rec={r} />
      </div>,
    );
    const listMarkers = markerHtml(list.container);

    expect(tileMarkers.priority).toBe(listMarkers.priority);
    expect(tileMarkers.ndp).toBe(listMarkers.ndp);
    expect(tileMarkers.ndp).not.toBe(""); // NDP rec carries the marker
  });
});
