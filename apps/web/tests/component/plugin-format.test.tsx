// Story 17.7 — Vitest coverage for the marketplace display helpers and the
// always-visible capability block.
//   UX-DR97: emoji collapse to a single ▒ glyph
//   UX-DR94: capability disclosure renders every capability, not collapsible

import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";

import { capabilityLabel, stripEmoji } from "@/lib/plugin-format";
import { CapabilityBlock } from "@/components/marketplace/capability-block";
import { VerifiedBadge } from "@/components/marketplace/verified-badge";
import type { PluginCapability } from "@/lib/api";

describe("stripEmoji (UX-DR97)", () => {
  it("replaces emoji with a single ▒ glyph", () => {
    expect(stripEmoji("SERP Enrichment 🚀")).toBe("SERP Enrichment ▒");
    expect(stripEmoji("Markdown Export ✨")).toBe("Markdown Export ▒");
  });

  it("leaves emoji-free names untouched", () => {
    expect(stripEmoji("ClickHouse Windowed Analytics")).toBe(
      "ClickHouse Windowed Analytics",
    );
  });
});

describe("capabilityLabel", () => {
  it("renders each capability kind deterministically", () => {
    expect(
      capabilityLabel({ kind: "network", allowlist: ["a.com", "b.com"] }),
    ).toBe("network → a.com, b.com");
    expect(capabilityLabel({ kind: "extractor-confidence-override" })).toBe(
      "extractor-confidence-override",
    );
  });
});

describe("CapabilityBlock (UX-DR94)", () => {
  it("renders every requested capability as an always-visible row", () => {
    const caps: PluginCapability[] = [
      { kind: "network", allowlist: ["api.x.com"] },
      { kind: "read-secret", keys: ["TOKEN"] },
    ];
    render(<CapabilityBlock capabilities={caps} />);
    const rows = screen.getAllByTestId("plugin-capability");
    expect(rows).toHaveLength(2);
    // No <details>/<summary> — the block is not collapsible.
    expect(document.querySelector("details")).toBeNull();
  });

  it("shows a 'none' notice when no capabilities are requested", () => {
    render(<CapabilityBlock capabilities={[]} />);
    expect(screen.getByTestId("plugin-capabilities")).toHaveTextContent(
      /none/i,
    );
  });
});

describe("VerifiedBadge (UX-DR90)", () => {
  it("renders a distinct chip for verified vs unverified vs revoked", () => {
    const { rerender } = render(
      <VerifiedBadge verified signature_status="signed" />,
    );
    expect(screen.getByTestId("plugin-trust")).toHaveAttribute(
      "data-trust",
      "verified",
    );

    rerender(<VerifiedBadge verified={false} signature_status="unsigned" />);
    expect(screen.getByTestId("plugin-trust")).toHaveAttribute(
      "data-trust",
      "unverified",
    );

    rerender(<VerifiedBadge verified signature_status="revoked" />);
    expect(screen.getByTestId("plugin-trust")).toHaveAttribute(
      "data-trust",
      "revoked",
    );
  });
});
