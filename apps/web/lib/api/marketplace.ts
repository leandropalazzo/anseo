// Story 41.3 — marketplace browse + plugin detail + install, wired to the LIVE
// registry.
//
// Epic 17 (17.7/17.8) shipped this surface against a hardcoded mock catalog
// (`done(mock)`). Story 41.1 landed the live GitHub flat-file registry client
// and Story 41.3 exposes it over HTTP (`GET /v1/marketplace/plugins`,
// `GET /v1/plugins`, `POST /v1/plugins/install`, `DELETE /v1/plugins/:id`,
// `POST /v1/plugins/:id/upgrade`). The mock is GONE: these fetchers read the
// real registry. Reads run server-side via `getJson` (the API key is attached
// from server env); the client-triggered install POST goes through the
// same-origin `/api/plugins/*` proxy so the key is never exposed to the browser.

import { getJson } from "./_client";

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

/**
 * Live registry catalog, merged with installed state. Returns an empty list
 * (never throws) when the registry is offline/empty so the page renders its
 * zero-state (Story 41.3 AC4) instead of erroring.
 */
export async function fetchMarketplacePlugins(): Promise<MarketplacePlugin[]> {
  const r = await getJson<{ plugins: MarketplacePlugin[] }>(
    "/v1/marketplace/plugins",
  );
  return r.plugins ?? [];
}

/**
 * Detail for one plugin by slug. The registry exposes no per-slug endpoint
 * (the catalog is small — Story 41.3 filters client/server-side over the
 * fetched index), so this resolves against the full marketplace list.
 */
export async function fetchMarketplacePlugin(
  slug: string,
): Promise<MarketplacePlugin | null> {
  const plugins = await fetchMarketplacePlugins();
  return plugins.find((p) => p.slug === slug) ?? null;
}

// ─── Install (Story 41.3 — live install via same-origin proxy) ───────────────
//
// Installs run server-side through the `/api/plugins/install` proxy, which
// attaches the operator API key and forwards to `POST /v1/plugins/install`.
// The API verifies the registry artifact's checksum + Ed25519 signature
// (Story 41.1 pipeline) and records the install in the audit table. The Sheet
// still enforces install discipline client-side: permissions acknowledged
// before [INSTALL →] (UX-DR91), explicit unsigned confirmation (UX-DR101), and
// a structured signing-failure error (UX-DR92).

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
    const r = await fetch("/api/plugins/install", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        id: slug,
        acknowledge_unsigned: req.acknowledge_unsigned,
      }),
      cache: "no-store",
    });
    return (await r.json()) as InstallResult;
  } catch {
    return {
      ok: false,
      signature_status: "unsigned",
      error_kind: "network",
      message: "Couldn't reach the registry to complete the install.",
    };
  }
}
