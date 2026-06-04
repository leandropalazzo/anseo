// Story 17.9 — dev-mode + Hosted Cloud gating for the /dev plugin-author
// surfaces. Both flags are read from NEXT_PUBLIC_* env so the sidebar
// (client component, UX-DR120) and the server-rendered /dev route agree.
//
//   UX-DR120 — /dev + its sidebar item are conditional on dev mode.
//   UX-DR124 — /dev refuses to render on Hosted Cloud (Phase 4 stub).

export function isDevModeEnabled(): boolean {
  return process.env.NEXT_PUBLIC_OGEO_DEV_MODE === "1";
}

export function isHostedCloud(): boolean {
  return process.env.NEXT_PUBLIC_OGEO_HOSTED_CLOUD === "1";
}

export type PluginLogLine = {
  at: string;
  level: "info" | "warn" | "error";
  message: string;
};

export type CapabilityUsage = {
  capability: string;
  declared: boolean;
  used: boolean;
};

export interface DevPluginState {
  plugin_slug: string;
  loaded_version: string;
  in_flight_invocations: number;
  logs: PluginLogLine[];
  capabilities: CapabilityUsage[];
}

// Mock dev state — the dev worker exposes this over a local socket in a real
// dev session; the dashboard reads a fixture until that seam lands.
export const DEV_PLUGIN_MOCK: DevPluginState = {
  plugin_slug: "local/dev-extractor",
  loaded_version: "0.1.0-dev+abc1234",
  in_flight_invocations: 2,
  logs: [
    { at: "2026-05-30T20:00:01Z", level: "info", message: "plugin loaded" },
    { at: "2026-05-30T20:00:03Z", level: "info", message: "extract() ok" },
    {
      at: "2026-05-30T20:00:05Z",
      level: "warn",
      message: "confidence override clamped to 1.0",
    },
  ],
  capabilities: [
    { capability: "network", declared: true, used: true },
    { capability: "read-secret", declared: true, used: false },
    { capability: "emit-event", declared: false, used: true },
  ],
};
