"use client";

import { useEffect, useState } from "react";

import { Icon, ICON_DEFAULTS } from "@/lib/icons";

export type Deployment = "local" | "cloud";

/** Shape of `GET /v1/serve/status` (Story 37.1), via the `/api/serve/status` proxy. */
interface ServeStatus {
  mode: "supervisor" | "standalone";
  tier: string;
}

/** Tier label shown until the live status resolves (and the SSR fallback). */
const DEFAULT_TIER = "local";

/**
 * Map the backend serve tier onto the two visual deployment glyphs. Anything
 * cloud-hosted (`cloud`, `enterprise`) gets the Cloud glyph; `local` /
 * `standalone` / unknown tiers render as a locally-running Server.
 */
function deploymentForTier(tier: string): Deployment {
  return tier === "cloud" || tier === "enterprise" ? "cloud" : "local";
}

/** Live serve tier, fetched once on mount from the same-origin proxy. */
function useServeTier(): string {
  const [tier, setTier] = useState<string>(DEFAULT_TIER);

  useEffect(() => {
    let cancelled = false;
    fetch("/api/serve/status", { cache: "no-store" })
      .then((r) => (r.ok ? r.json() : Promise.reject(r.status)))
      .then((s: ServeStatus) => {
        if (!cancelled && s?.tier) setTier(s.tier);
      })
      .catch(() => {
        /* Backend unreachable — keep the local default rather than blanking. */
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return tier;
}

/**
 * Live deployment hint for components that only need the glyph variant
 * (Sidebar's ProjectSwitcher). Driven by the backend serve tier, not a
 * hardcoded constant (Story 46.3).
 */
export function useDeployment(): Deployment {
  return deploymentForTier(useServeTier());
}

/**
 * Live indicator of the active deployment in the topbar. Not a control — it
 * reflects where the dashboard's backend is running, read from
 * `GET /v1/serve/status` (Story 46.3, was a hardcoded "local").
 */
export function DeploymentSwitch() {
  const tier = useServeTier();
  const deployment = deploymentForTier(tier);
  const Glyph = deployment === "local" ? Icon.Server : Icon.Cloud;
  return (
    <div
      data-testid="deployment-indicator"
      aria-label={`Deployment: ${tier}`}
      className="inline-flex items-center gap-[6px] border border-[color:var(--border)] bg-[color:var(--bg-sunken)] px-[8px] py-[4px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] uppercase tracking-[0.06em] text-[color:var(--text-muted)]"
    >
      <Glyph size={11} strokeWidth={ICON_DEFAULTS.strokeWidth} />
      {tier}
    </div>
  );
}
