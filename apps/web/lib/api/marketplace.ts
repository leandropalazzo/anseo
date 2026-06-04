// Story 17.7 marketplace browse + plugin detail (mock-backed) and
// Story 17.8 marketplace install Sheet (mock-backed).
//
// Story 17.5 ships the registry as a filesystem/GitHub-backed store with no
// HTTP surface, so the dashboard reads a mock catalog whose shape mirrors the
// plugin-manifest crate + IndexEntry + PluginInstallRow. The fetchers probe an
// expected `/v1/plugins*` route first and fall back to the mock so the live
// swap is a no-op once the API lands.

import { API_BASE_URL, getJson, setupHeaders } from "./_client";

export type PluginType =
  | "provider"
  | "extractor"
  | "analytics"
  | "output-format";

export type PluginCapability =
  | { kind: "network"; allowlist: string[] }
  | { kind: "read-secret"; keys: string[] }
  | { kind: "emit-event"; kinds: string[] }
  | { kind: "extractor-confidence-override" }
  | { kind: "analytics-window"; windows: string[] };

export type PluginSignatureStatus = "signed" | "unsigned" | "revoked";

export interface MarketplacePlugin {
  /** namespace/name, used as the detail-route slug. */
  slug: string;
  name: string;
  /** Always an explicit pinned version — never an implicit `latest`. */
  version: string;
  description: string;
  author: string;
  homepage: string;
  plugin_type: PluginType;
  /** Verified-publisher badge (UX-DR90). */
  verified: boolean;
  signature_status: PluginSignatureStatus;
  capabilities: PluginCapability[];
  installed: boolean;
  installed_version?: string;
  /** Drives the update-available indicator (UX-DR100). */
  update_available: boolean;
}

export async function fetchMarketplacePlugins(): Promise<MarketplacePlugin[]> {
  try {
    const r = await getJson<{ plugins: MarketplacePlugin[] }>("/v1/plugins");
    return r.plugins;
  } catch {
    const { MARKETPLACE_MOCK } = await import("../marketplace-mock");
    return MARKETPLACE_MOCK;
  }
}

export async function fetchMarketplacePlugin(
  slug: string,
): Promise<MarketplacePlugin | null> {
  try {
    return await getJson<MarketplacePlugin>(
      `/v1/plugins/${encodeURIComponent(slug)}`,
    );
  } catch {
    const { MARKETPLACE_MOCK } = await import("../marketplace-mock");
    return MARKETPLACE_MOCK.find((p) => p.slug === slug) ?? null;
  }
}

// ─── Story 17.8 marketplace install Sheet (mock-backed) ──────────────────────
//
// Installs are performed by the `ogeo` CLI; there's no HTTP install surface
// yet, so this POST is mock-backed. The Sheet still enforces the real install
// discipline client-side: permissions acknowledged before [INSTALL →]
// (UX-DR91), all-or-nothing (OQ-P3-26), explicit unsigned confirmation
// (UX-DR101), and a structured signing-failure error (UX-DR92). On success the
// backend records a `plugin_install` Audit Event (UX-DR95/127) — surfaced here
// as the returned audit_event_id.

export type InstallErrorKind =
  | "signing_failed"
  | "capability_denied"
  | "revoked"
  | "network";

export interface InstallRequest {
  /** Operator acknowledged installing an unsigned plugin (UX-DR101). */
  acknowledge_unsigned: boolean;
}

export interface InstallResult {
  ok: boolean;
  signature_status: PluginSignatureStatus;
  /** Set on success — the plugin_install Audit Event id (UX-DR95/127). */
  audit_event_id?: string;
  error_kind?: InstallErrorKind;
  message: string;
}

export async function installPlugin(
  slug: string,
  req: InstallRequest,
): Promise<InstallResult> {
  try {
    const r = await fetch(
      `${API_BASE_URL}/v1/plugins/${encodeURIComponent(slug)}/install`,
      {
        method: "POST",
        headers: await setupHeaders(true),
        body: JSON.stringify(req),
        cache: "no-store",
      },
    );
    return (await r.json()) as InstallResult;
  } catch {
    // Mock fallback: signed plugins install clean; unsigned require the
    // acknowledgment flag or the install is rejected as a signing failure.
    const { MARKETPLACE_MOCK } = await import("../marketplace-mock");
    const plugin = MARKETPLACE_MOCK.find((p) => p.slug === slug);
    const status = plugin?.signature_status ?? "unsigned";
    if (status === "revoked") {
      return {
        ok: false,
        signature_status: "revoked",
        error_kind: "revoked",
        message: "Publisher key has been revoked; install blocked.",
      };
    }
    if (status === "unsigned" && !req.acknowledge_unsigned) {
      return {
        ok: false,
        signature_status: "unsigned",
        error_kind: "signing_failed",
        message: "Unsigned plugin — install not acknowledged.",
      };
    }
    return {
      ok: true,
      signature_status: status,
      audit_event_id: `evt_${slug.replace(/\W+/g, "_")}`,
      message: "Plugin installed.",
    };
  }
}
