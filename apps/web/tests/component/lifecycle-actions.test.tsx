// Story 19.8 — Vitest coverage for the recommendation lifecycle action bar.
// Pins Snooze→acknowledged vs Dismiss→dismissed (UX-DR104) and the
// acted-without-measurement loop flag (UX-DR108), with the API mocked.

import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";

import { LifecycleActions } from "@/app/recommendations/[id]/_components/lifecycle-actions";
import type { Recommendation } from "@/lib/api";

const refresh = vi.fn();
vi.mock("next/navigation", () => ({
  useRouter: () => ({ refresh }),
}));

const transition = vi.fn();
vi.mock("@/lib/api", async (orig) => {
  const actual = await orig<typeof import("@/lib/api")>();
  return {
    ...actual,
    transitionRecommendation: (...a: unknown[]) => transition(...(a as [])),
  };
});

beforeEach(() => {
  refresh.mockClear();
  transition.mockReset();
  transition.mockResolvedValue({ recommendation: {}, warnings: [] });
});

function rec(overrides: Partial<Recommendation> = {}): Recommendation {
  return {
    id: "01JABCDEF0123456789ABCDEFG",
    project_id: "01PROJECT0000000000000000",
    kind: "visibility_gap",
    severity: "high",
    confidence_band: "medium",
    state: "surfaced",
    summary: "x",
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
    reproducibility: { class: "byte_stable", note: null },
    tags: [],
    generated_at: "2026-05-30T00:00:00Z",
    engine_version: "v",
    ...overrides,
  };
}

describe("LifecycleActions", () => {
  it("maps Snooze to acknowledged (UX-DR104)", async () => {
    render(<LifecycleActions rec={rec()} />);
    fireEvent.click(screen.getByTestId("rec-action-snooze"));
    await waitFor(() =>
      expect(transition).toHaveBeenCalledWith(rec().id, {
        to: "acknowledged",
        evidence_url: undefined,
      }),
    );
  });

  it("maps Dismiss to dismissed (UX-DR104)", async () => {
    render(<LifecycleActions rec={rec()} />);
    fireEvent.click(screen.getByTestId("rec-action-dismiss"));
    await waitFor(() =>
      expect(transition).toHaveBeenCalledWith(rec().id, {
        to: "dismissed",
        evidence_url: undefined,
      }),
    );
  });

  it("flags the measurement loop while in acted state (UX-DR108)", () => {
    render(<LifecycleActions rec={rec({ state: "acted" })} />);
    expect(screen.getByTestId("rec-measurement-flag")).toBeInTheDocument();
  });

  it("disables actions in a terminal state", () => {
    render(<LifecycleActions rec={rec({ state: "dismissed" })} />);
    expect(screen.getByTestId("rec-action-snooze")).toBeDisabled();
    expect(screen.getByTestId("rec-action-dismiss")).toBeDisabled();
  });
});
