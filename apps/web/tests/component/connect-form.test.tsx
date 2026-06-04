// Story 15.4 — Vitest coverage for the ClickHouse remote-connect form.
// Pins preset auto-fill (OQ-P3-23) + the structured error-banner rendering
// for each failure state, without a backend. The Playwright walkthrough
// covers the live submit → /setup redirect.

import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { ConnectForm } from "@/app/setup/clickhouse/connect/_components/connect-form";
import type { ConnectResult } from "@/lib/api";

const push = vi.fn();
const refresh = vi.fn();
vi.mock("next/navigation", () => ({
  useRouter: () => ({ push, refresh }),
}));

const postConnect = vi.fn<() => Promise<ConnectResult>>();
vi.mock("@/lib/api", async (orig) => {
  const actual = await orig<typeof import("@/lib/api")>();
  return { ...actual, postClickHouseConnect: (...a: unknown[]) => postConnect(...(a as [])) };
});

beforeEach(() => {
  push.mockClear();
  refresh.mockClear();
  postConnect.mockReset();
});

describe("ConnectForm", () => {
  it("auto-fills the canonical origin URL when a preset is chosen", () => {
    render(<ConnectForm />);
    const endpoint = screen.getByTestId("ch-endpoint-input") as HTMLInputElement;
    // Default preset is ClickHouse Cloud.
    expect(endpoint.value).toContain("clickhouse.cloud");

    fireEvent.click(
      screen.getByTestId("ch-preset-tinybird").querySelector("input")!,
    );
    expect(endpoint.value).toBe("https://api.tinybird.co");

    fireEvent.click(
      screen.getByTestId("ch-preset-custom").querySelector("input")!,
    );
    expect(endpoint.value).toBe("");
  });

  it("renders the invalid-credentials banner on a failed probe", async () => {
    postConnect.mockResolvedValue({
      ok: false,
      state: "invalid_credentials",
      message: "rejected",
    });
    render(<ConnectForm />);
    fireEvent.click(screen.getByTestId("ch-connect-submit"));

    const banner = await screen.findByTestId("ch-connect-error");
    expect(banner).toHaveAttribute("data-state", "invalid_credentials");
    expect(banner).toHaveTextContent(/rejected those credentials/i);
    expect(push).not.toHaveBeenCalled();
  });

  it("redirects to /setup on a successful connect", async () => {
    postConnect.mockResolvedValue({
      ok: true,
      state: "connected",
      message: "ok",
      endpoint: "https://abc.clickhouse.cloud:8443",
    });
    render(<ConnectForm />);
    fireEvent.click(screen.getByTestId("ch-connect-submit"));

    await waitFor(() => expect(push).toHaveBeenCalledWith("/setup"));
    expect(screen.queryByTestId("ch-connect-error")).not.toBeInTheDocument();
  });

  it("maps every failure state to operator copy", async () => {
    postConnect.mockResolvedValue({
      ok: false,
      state: "unreachable",
      message: "raw",
    });
    render(<ConnectForm />);
    fireEvent.click(screen.getByTestId("ch-connect-submit"));
    const banner = await screen.findByTestId("ch-connect-error");
    expect(banner).toHaveTextContent(/Couldn't reach/i);
  });
});
