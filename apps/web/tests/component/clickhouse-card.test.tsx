// Story 15.3 — Vitest coverage for the ClickHouseCard Docker-detect branches.
// The Playwright walkthrough covers the live install SSE flow; these unit
// tests pin the pure render branches (present / too-old / absent) without a
// backend, since the Docker verdict drives whether install vs. remote-connect
// is offered (AC-1, AC-4).

import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { ClickHouseCard } from "@/app/setup/_components/clickhouse-card";
import type { SetupStatus } from "@/lib/api";

const push = vi.fn();
const refresh = vi.fn();
vi.mock("next/navigation", () => ({
  useRouter: () => ({ push, refresh }),
}));

// The card imports install helpers; stub them so the module loads under jsdom.
vi.mock("@/lib/api", async (orig) => {
  const actual = await orig<typeof import("@/lib/api")>();
  return {
    ...actual,
    postClickHouseInstall: vi.fn(),
    streamClickHouseInstall: vi.fn(),
  };
});

const ch: SetupStatus["clickhouse"] = {
  state: "not_configured",
  url: null,
  row_count: null,
  etl_lag_seconds: null,
};

describe("ClickHouseCard Docker verdict", () => {
  it("offers local install when Docker is present and modern", () => {
    render(
      <ClickHouseCard
        clickhouse={ch}
        docker={{ present: true, version: "24.0.7" }}
      />,
    );
    expect(screen.getByTestId("ch-docker-verdict")).toHaveAttribute(
      "data-verdict",
      "present",
    );
    expect(screen.getByTestId("ch-install-button")).toBeInTheDocument();
    expect(
      screen.queryByTestId("ch-remote-connect-button"),
    ).not.toBeInTheDocument();
  });

  it("routes to remote-connect when Docker is absent", () => {
    render(
      <ClickHouseCard
        clickhouse={ch}
        docker={{ present: false, version: null }}
      />,
    );
    expect(screen.getByTestId("ch-docker-verdict")).toHaveAttribute(
      "data-verdict",
      "absent",
    );
    expect(screen.queryByTestId("ch-install-button")).not.toBeInTheDocument();
    const cta = screen.getByTestId("ch-remote-connect-cta");
    expect(cta).toHaveTextContent("Docker isn't available");
    expect(screen.getByTestId("ch-remote-connect-button")).toBeInTheDocument();
  });

  it("treats an old Docker engine as too-old and hides local install", () => {
    render(
      <ClickHouseCard
        clickhouse={ch}
        docker={{ present: true, version: "18.09.1" }}
      />,
    );
    expect(screen.getByTestId("ch-docker-verdict")).toHaveAttribute(
      "data-verdict",
      "too_old",
    );
    expect(screen.queryByTestId("ch-install-button")).not.toBeInTheDocument();
    expect(screen.getByTestId("ch-remote-connect-button")).toBeInTheDocument();
  });
});
