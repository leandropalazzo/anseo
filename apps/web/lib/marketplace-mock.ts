// Story 17.7 — mock plugin catalog. Shape mirrors the plugin-manifest crate
// (PluginManifest + Capability) plus the registry IndexEntry and the
// PluginInstallRow installed-state fields. Swapped for the live `/v1/plugins`
// response once the API surface lands (Story 17.5 is filesystem-backed only).

import type { MarketplacePlugin } from "./api";

export const MARKETPLACE_MOCK: MarketplacePlugin[] = [
  {
    slug: "opengeo/serp-enrichment",
    name: "SERP Enrichment 🚀",
    version: "1.4.2",
    description:
      "Enriches prompt runs with live search-result snippets for citation grounding.",
    author: "OpenGEO Labs",
    homepage: "https://github.com/opengeo/serp-enrichment",
    plugin_type: "extractor",
    verified: true,
    signature_status: "signed",
    capabilities: [
      { kind: "network", allowlist: ["api.serpprovider.com"] },
      { kind: "extractor-confidence-override" },
    ],
    installed: true,
    installed_version: "1.4.0",
    update_available: true,
  },
  {
    slug: "community/markdown-export",
    name: "Markdown Export ✨",
    version: "0.9.0",
    description: "Renders recommendation digests as portable Markdown reports.",
    author: "jane-doe",
    homepage: "https://example.com/markdown-export",
    plugin_type: "output-format",
    verified: false,
    signature_status: "unsigned",
    capabilities: [{ kind: "emit-event", kinds: ["report.generated"] }],
    installed: false,
    update_available: false,
  },
  {
    slug: "opengeo/clickhouse-window",
    name: "ClickHouse Windowed Analytics",
    version: "2.0.1",
    description:
      "Adds rolling-window analytics aggregations backed by ClickHouse.",
    author: "OpenGEO Labs",
    homepage: "https://github.com/opengeo/clickhouse-window",
    plugin_type: "analytics",
    verified: true,
    signature_status: "signed",
    capabilities: [
      { kind: "analytics-window", windows: ["7d", "28d", "90d"] },
      { kind: "read-secret", keys: ["CLICKHOUSE_DSN"] },
    ],
    installed: true,
    installed_version: "2.0.1",
    update_available: false,
  },
];
