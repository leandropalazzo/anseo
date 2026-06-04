// Story 17.8 — Vitest coverage for the install Sheet gate + unsigned flow.
//   UX-DR91/OQ-P3-26: [INSTALL →] disabled until the all-or-nothing
//                     acknowledgment is checked.
//   UX-DR101: unsigned plugins route through the ⚠ confirmation Dialog.
//   UX-DR92:  signing failure renders a structured ErrorBanner.
//   UX-DR95:  success surfaces the Audit Event id.

import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";

import { InstallSheet } from "@/components/marketplace/install-sheet";
import type { InstallResult, MarketplacePlugin } from "@/lib/api";

const install = vi.fn<() => Promise<InstallResult>>();
vi.mock("@/lib/api", async (orig) => {
  const actual = await orig<typeof import("@/lib/api")>();
  return { ...actual, installPlugin: (...a: unknown[]) => install(...(a as [])) };
});

beforeEach(() => install.mockReset());

function plugin(overrides: Partial<MarketplacePlugin> = {}): MarketplacePlugin {
  return {
    slug: "community/markdown-export",
    name: "Markdown Export",
    version: "0.9.0",
    description: "x",
    author: "jane",
    homepage: "https://example.com",
    plugin_type: "output-format",
    verified: false,
    signature_status: "signed",
    capabilities: [{ kind: "emit-event", kinds: ["report.generated"] }],
    installed: false,
    update_available: false,
    ...overrides,
  };
}

describe("InstallSheet", () => {
  it("gates [INSTALL →] behind the permissions acknowledgment (UX-DR91)", () => {
    render(<InstallSheet plugin={plugin()} />);
    fireEvent.click(screen.getByTestId("open-install-sheet"));
    // Permissions block renders before the install action.
    expect(screen.getByTestId("install-permissions")).toBeInTheDocument();
    expect(screen.getByTestId("install-confirm")).toBeDisabled();
    fireEvent.click(screen.getByTestId("install-acknowledge"));
    expect(screen.getByTestId("install-confirm")).toBeEnabled();
  });

  it("routes unsigned installs through the ⚠ Dialog (UX-DR101)", async () => {
    install.mockResolvedValue({
      ok: true,
      signature_status: "unsigned",
      audit_event_id: "evt_1",
      message: "ok",
    });
    render(<InstallSheet plugin={plugin({ signature_status: "unsigned" })} />);
    fireEvent.click(screen.getByTestId("open-install-sheet"));
    fireEvent.click(screen.getByTestId("install-acknowledge"));
    fireEvent.click(screen.getByTestId("install-confirm"));

    expect(screen.getByTestId("unsigned-dialog")).toBeInTheDocument();
    expect(install).not.toHaveBeenCalled();

    fireEvent.click(screen.getByTestId("unsigned-confirm"));
    await waitFor(() =>
      expect(install).toHaveBeenCalledWith("community/markdown-export", {
        acknowledge_unsigned: true,
      }),
    );
    expect(await screen.findByTestId("install-audit-event")).toHaveTextContent(
      "evt_1",
    );
  });

  it("renders a structured error on signing failure (UX-DR92)", async () => {
    install.mockResolvedValue({
      ok: false,
      signature_status: "signed",
      error_kind: "signing_failed",
      message: "raw",
    });
    render(<InstallSheet plugin={plugin()} />);
    fireEvent.click(screen.getByTestId("open-install-sheet"));
    fireEvent.click(screen.getByTestId("install-acknowledge"));
    fireEvent.click(screen.getByTestId("install-confirm"));

    const banner = await screen.findByTestId("install-error");
    expect(banner).toHaveAttribute("data-error-kind", "signing_failed");
  });

  it("blocks installing a revoked plugin", () => {
    render(<InstallSheet plugin={plugin({ signature_status: "revoked" })} />);
    expect(screen.getByTestId("open-install-sheet")).toBeDisabled();
  });
});
