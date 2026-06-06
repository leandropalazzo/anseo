// Story 47.4 — Vitest coverage for the operator analytics panels.
//
// Focus (AC-8): the funnel chart's drop-off rendering, including the
// "tracking deployed mid-funnel" anomaly (later step has MORE events → N/A, not
// a negative percentage). Also covers the verify funnel empty/populated states
// and the ranked-list empty state, plus the page-level emptiness helper.

import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";

import { FunnelChart } from "@/app/analytics/_components/funnel-chart";
import { VerifyFunnel } from "@/app/analytics/_components/verify-funnel";
import { RankedList } from "@/app/analytics/_components/ranked-list";
import {
  isAnalyticsEmpty,
  normalizePeriod,
  type Funnels,
  type SiteOverview,
} from "@/lib/api";

describe("FunnelChart drop-off", () => {
  it("renders drop-off percentages between steps", () => {
    render(
      <FunnelChart
        steps={[
          { label: "contribute_start", count: 100, drop_off_pct: null },
          { label: "contribute_step", count: 62, drop_off_pct: 38.0 },
          { label: "contribute_complete", count: 41, drop_off_pct: 33.9 },
        ]}
      />,
    );
    // First step has no drop-off marker.
    expect(screen.queryByTestId("funnel-dropoff-0")).toBeNull();
    expect(screen.getByTestId("funnel-dropoff-1").textContent).toContain("38.0%");
    expect(screen.getByTestId("funnel-dropoff-2").textContent).toContain("33.9%");
  });

  it("renders N/A (never a negative %) when a later step grew", () => {
    render(
      <FunnelChart
        steps={[
          { label: "start", count: 10, drop_off_pct: null },
          { label: "step", count: 25, drop_off_pct: null }, // grew → API sends null
          { label: "complete", count: 5, drop_off_pct: 80.0 },
        ]}
      />,
    );
    const grew = screen.getByTestId("funnel-dropoff-1").textContent ?? "";
    expect(grew).toContain("N/A");
    expect(grew).not.toContain("-");
    expect(grew).not.toContain("−");
    expect(screen.getByTestId("funnel-dropoff-2").textContent).toContain("80.0%");
  });

  it("escapes a malicious step label as plain text (no markup injection)", () => {
    render(
      <FunnelChart
        steps={[
          { label: "<img src=x onerror=alert(1)>", count: 5, drop_off_pct: null },
        ]}
      />,
    );
    // Rendered verbatim as text, never parsed into an <img> element.
    expect(screen.queryByRole("img")).toBeNull();
    expect(
      screen.getByText("<img src=x onerror=alert(1)>"),
    ).toBeInTheDocument();
  });
});

describe("VerifyFunnel", () => {
  it("shows an empty state with no methods", () => {
    render(<VerifyFunnel methods={[]} />);
    expect(screen.getByText("No verification activity")).toBeInTheDocument();
  });

  it("renders per-method rows with success rate and N/A on zero starts", () => {
    render(
      <VerifyFunnel
        methods={[
          { method: "dns", start: 10, complete: 7, fail: 3, success_rate_pct: 70.0 },
          { method: "email", start: 0, complete: 0, fail: 0, success_rate_pct: null },
        ]}
      />,
    );
    expect(screen.getByTestId("verify-row-dns").textContent).toContain("70.0%");
    expect(screen.getByTestId("verify-row-email").textContent).toContain("—");
  });
});

describe("RankedList", () => {
  it("shows the empty state when there are no rows", () => {
    render(
      <RankedList
        rows={[]}
        unitLabel="views"
        emptyTitle="No page views yet"
        emptyHint="hint"
      />,
    );
    expect(screen.getByText("No page views yet")).toBeInTheDocument();
  });
});

describe("analytics helpers", () => {
  it("normalizePeriod only honors 7d/30d", () => {
    expect(normalizePeriod("30d")).toBe("30d");
    expect(normalizePeriod("7d")).toBe("7d");
    expect(normalizePeriod(undefined)).toBe("7d");
    expect(normalizePeriod("garbage")).toBe("7d");
  });

  it("isAnalyticsEmpty is true only when every panel is empty", () => {
    const emptyOverview: SiteOverview = {
      period_days: 7,
      sessions_per_day: [],
      top_pages: [],
      top_referrers: [],
    };
    const emptyFunnels: Funnels = {
      period_days: 7,
      contribute: [
        { label: "contribute_start", count: 0, drop_off_pct: null },
      ],
      verify: [],
      badge_embeds_per_day: [],
    };
    expect(isAnalyticsEmpty(emptyOverview, emptyFunnels)).toBe(true);

    const withData: Funnels = {
      ...emptyFunnels,
      badge_embeds_per_day: [{ date: "2026-06-06", count: 3 }],
    };
    expect(isAnalyticsEmpty(emptyOverview, withData)).toBe(false);
  });
});
