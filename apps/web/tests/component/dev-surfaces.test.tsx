// Story 17.9 — Vitest coverage for the /dev plugin-author surfaces.
//   UX-DR119/OQ-P3-28: dev banner unmistakable + per-session dismissal
//   UX-DR123: capability inspector declared-vs-used diff
//   UX-DR121/125: hot-reload flips version atomically; in-flight preserved
//   UX-DR122: logs are append-only (lines only ever added)

import { describe, it, expect, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";

import { DevBanner } from "@/components/dev/dev-banner";
import {
  CapabilityInspector,
  diffState,
} from "@/components/dev/capability-inspector";
import { DevOverview } from "@/app/dev/_components/dev-overview";
import type { CapabilityUsage, DevPluginState } from "@/lib/dev-mode";

beforeEach(() => sessionStorage.clear());

describe("diffState (UX-DR123)", () => {
  it("classifies declared/used combinations", () => {
    expect(diffState({ capability: "n", declared: true, used: true })).toBe(
      "ok",
    );
    expect(diffState({ capability: "n", declared: true, used: false })).toBe(
      "unused",
    );
    expect(diffState({ capability: "n", declared: false, used: true })).toBe(
      "undeclared",
    );
  });
});

describe("CapabilityInspector", () => {
  it("flags an undeclared-but-used capability as a violation", () => {
    const caps: CapabilityUsage[] = [
      { capability: "emit-event", declared: false, used: true },
    ];
    render(<CapabilityInspector capabilities={caps} />);
    expect(screen.getByTestId("capability-row")).toHaveAttribute(
      "data-diff",
      "undeclared",
    );
  });
});

describe("DevBanner (UX-DR119/OQ-P3-28)", () => {
  it("renders by default and hides after a per-session dismiss", () => {
    render(<DevBanner />);
    expect(screen.getByTestId("dev-mode-banner")).toBeInTheDocument();
    fireEvent.click(screen.getByTestId("dev-banner-dismiss"));
    expect(screen.queryByTestId("dev-mode-banner")).not.toBeInTheDocument();
    expect(sessionStorage.getItem("ogeo-dev-banner-dismissed")).toBe("1");
  });
});

function devState(): DevPluginState {
  return {
    plugin_slug: "local/dev-extractor",
    loaded_version: "0.1.0-dev+abc1234",
    in_flight_invocations: 2,
    logs: [{ at: "2026-05-30T20:00:00Z", level: "info", message: "loaded" }],
    capabilities: [{ capability: "network", declared: true, used: true }],
  };
}

describe("DevOverview hot-reload (UX-DR121/122/125)", () => {
  it("flips the version atomically and only appends logs", async () => {
    render(<DevOverview state={devState()} />);
    const before = screen.getByTestId("dev-loaded-version").textContent;
    const logsBefore = screen.getAllByTestId("dev-log-line").length;

    fireEvent.click(screen.getByTestId("dev-hot-reload"));
    // Mid-reload the in-flight count on the old version is surfaced.
    expect(screen.getByTestId("dev-in-flight")).toBeInTheDocument();

    await waitFor(() =>
      expect(screen.getByTestId("dev-loaded-version").textContent).not.toBe(
        before,
      ),
    );
    // Append-only: log count strictly increased.
    expect(screen.getAllByTestId("dev-log-line").length).toBeGreaterThan(
      logsBefore,
    );
  });
});
