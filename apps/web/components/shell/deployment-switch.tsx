"use client";

import { Icon, ICON_DEFAULTS } from "@/lib/icons";

export type Deployment = "local" | "cloud";

// This build targets a local deployment only — there is no cloud backend yet,
// so the topbar shows where we're running rather than offering a toggle.
const CURRENT: Deployment = "local";

/** Read-only hook so other components (Sidebar's ProjectSwitcher) can react. */
export function useDeployment(): Deployment {
  return CURRENT;
}

/**
 * Static indicator of the active deployment in the topbar. Not a control —
 * it reflects where the dashboard is running (currently always Local).
 */
export function DeploymentSwitch() {
  return (
    <div
      data-testid="deployment-indicator"
      aria-label={`Deployment: ${CURRENT}`}
      className="inline-flex items-center gap-[6px] border border-[color:var(--border)] bg-[color:var(--bg-sunken)] px-[8px] py-[4px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] uppercase tracking-[0.06em] text-[color:var(--text-muted)]"
    >
      <Icon.Server size={11} strokeWidth={ICON_DEFAULTS.strokeWidth} />
      {CURRENT}
    </div>
  );
}
