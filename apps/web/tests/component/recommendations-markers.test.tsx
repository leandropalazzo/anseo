// Story 19.8 — Vitest coverage for the recommendation priority label, the NDP
// marker (UX-DR106), and the evidence chips (UX-DR105/103). The Playwright
// walkthrough covers the live list/detail render + Axe.

import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";

import {
  NdpMarkerFor,
  PriorityLabel,
} from "@/app/recommendations/_components/priority-label";
import { EvidenceChips } from "@/app/recommendations/_components/evidence-chip";
import type {
  Recommendation,
  RecommendationTraceability,
} from "@/lib/api";

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

function baseRec(overrides: Partial<Recommendation> = {}): Recommendation {
  return {
    id: "01JABCDEF0123456789ABCDEFG",
    project_id: "01PROJECT0000000000000000",
    kind: "visibility_gap",
    severity: "high",
    confidence_band: "medium",
    state: "surfaced",
    summary: "Brand visibility dropped on Perplexity",
    payload: {},
    traceability: emptyTrace(),
    reproducibility: { class: "byte_stable", note: null },
    tags: [],
    generated_at: "2026-05-30T00:00:00Z",
    engine_version: "sm14-1.0.0",
    ...overrides,
  };
}

function emptyTrace(): RecommendationTraceability {
  return {
    source_run_ids: [],
    source_run_ids_truncated: false,
    source_citation_ids: [],
    source_citation_ids_truncated: false,
    source_benchmark_queries: [],
    window: { start: "2026-05-01T00:00:00Z", end: "2026-05-30T00:00:00Z" },
    input_fingerprint: "abc123",
  };
}

describe("PriorityLabel", () => {
  it("renders the severity token plus a human label (UX-DR107)", () => {
    render(<PriorityLabel severity="high" />);
    const el = screen.getByTestId("rec-priority");
    expect(el).toHaveAttribute("data-severity", "high");
    expect(el).toHaveTextContent("high");
    expect(el).toHaveTextContent("High");
  });
});

describe("NdpMarkerFor", () => {
  it("shows the ⚠ NDP marker only for non-deterministic recs (UX-DR106)", () => {
    const { rerender } = render(
      <NdpMarkerFor
        rec={baseRec({
          reproducibility: { class: "non_deterministic", note: null },
        })}
      />,
    );
    expect(screen.getByTestId("rec-ndp-marker")).toHaveTextContent("NDP");

    rerender(
      <NdpMarkerFor
        rec={baseRec({ reproducibility: { class: "byte_stable", note: null } })}
      />,
    );
    expect(screen.queryByTestId("rec-ndp-marker")).not.toBeInTheDocument();
  });
});

describe("EvidenceChips", () => {
  it("links every source run + citation as a real anchor (UX-DR105)", () => {
    const trace = emptyTrace();
    trace.source_run_ids = ["01RUNAAAAAAAAAAAAAAAAAAAAA"];
    trace.source_citation_ids = ["01CITEBBBBBBBBBBBBBBBBBBBB"];
    render(<EvidenceChips traceability={trace} />);

    const run = screen.getByTestId("rec-evidence-run");
    expect(run).toHaveAttribute(
      "href",
      "/runs/01RUNAAAAAAAAAAAAAAAAAAAAA",
    );
    const cite = screen.getByTestId("rec-evidence-citation");
    expect(cite.getAttribute("href")).toContain("/citations#");
  });

  it("renders a render-error when traceability is empty (UX-DR103)", () => {
    render(<EvidenceChips traceability={emptyTrace()} />);
    expect(screen.getByTestId("rec-evidence-error")).toBeInTheDocument();
    expect(screen.queryByTestId("rec-evidence")).not.toBeInTheDocument();
  });
});
