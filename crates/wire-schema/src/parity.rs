//! Parity-as-CI-contract capability registry (Story 36.10, RISK-4).
//!
//! Parity — every capability reachable from CLI, Web, and MCP — is the v-next
//! differentiator, but historically it was asserted as a *value* with no
//! mechanism: three hand-maintained surfaces over one core, kept in sync by
//! reviewer vigilance alone. This module makes parity hold *by construction*.
//!
//! It declares a typed [`Capability`] registry. Each entry names a v-next
//! capability and records, per surface ({CLI, Web/API, MCP}), how it is
//! reached — or carries an explicit [`SingleSurfaceException`] documenting a
//! deliberate gap. The companion test in `tests/parity_contract.rs` fails CI if
//! a registered capability lacks a surface without an annotated exception, and
//! cross-checks the registry against the surfaces that are cheaply enumerable
//! from Rust (the MCP tool catalog and the OpenAPI path set).
//!
//! ## What this can and cannot assert
//! - **MCP** evidence ([`SurfaceCoverage::Mcp`]) is the canonical MCP tool name.
//!   The test cross-checks it against [`CANONICAL_MCP_TOOLS`], a read-only mirror
//!   of the closed tool catalog in `apps/mcp/src/tools/mod.rs::registry()`. That
//!   crate is an application binary, so wire-schema cannot depend on it; the
//!   mirror is pinned here and the count/names are asserted so drift surfaces.
//! - **Web/API** evidence ([`SurfaceCoverage::WebApi`]) is the `/v1` OpenAPI path
//!   (or, for management capabilities, the canonical Web route). For `/v1` paths
//!   the test cross-checks against [`KNOWN_V1_PATHS`] (mirror of the
//!   `gen-openapi` `build_spec()` path set). Web routes are TypeScript/Next.js
//!   and are *not* machine-asserted from Rust — they are recorded as documented
//!   evidence; the route-existence check lives in the web e2e suite.
//! - **CLI** evidence ([`SurfaceCoverage::Cli`]) is the canonical CLI verb path.
//!   The CLI `clap` enum lives in `apps/cli`, another binary wire-schema cannot
//!   depend on, so CLI verbs are recorded as documented evidence rather than
//!   machine-derived. (A future round could expose the verb list from a shared
//!   crate and tighten this.)
//!
//! The result is a *meaningful, maintainable, green* contract: internal
//! consistency is fully enforced, the two enumerable surfaces are cross-checked,
//! and the gaps that are not yet machine-assertable are documented in one place
//! rather than scattered across three surfaces.

/// A surface through which a capability can be exposed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Surface {
    /// The `opengeo` CLI (`apps/cli`, `clap` verb tree).
    Cli,
    /// The Web UI + its backing `/v1` REST API (`apps/web` + `apps/api`).
    WebApi,
    /// The MCP server tool catalog (`apps/mcp`).
    Mcp,
}

impl Surface {
    /// All three parity surfaces, in canonical order.
    pub const ALL: [Surface; 3] = [Surface::Cli, Surface::WebApi, Surface::Mcp];
}

/// How a single capability is reached on a single surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceCoverage {
    /// Reachable via the named CLI verb path, e.g. `opengeo recommend list`.
    Cli(&'static str),
    /// Reachable via the named Web route and/or `/v1` OpenAPI path.
    WebApi(&'static str),
    /// Reachable via the named MCP tool, e.g. `get_visibility`.
    Mcp(&'static str),
}

impl SurfaceCoverage {
    /// The surface this coverage belongs to.
    pub fn surface(&self) -> Surface {
        match self {
            SurfaceCoverage::Cli(_) => Surface::Cli,
            SurfaceCoverage::WebApi(_) => Surface::WebApi,
            SurfaceCoverage::Mcp(_) => Surface::Mcp,
        }
    }

    /// The evidence string (verb path, route/path, or tool name).
    pub fn evidence(&self) -> &'static str {
        match self {
            SurfaceCoverage::Cli(s) | SurfaceCoverage::WebApi(s) | SurfaceCoverage::Mcp(s) => s,
        }
    }
}

/// A deliberate, reviewed decision that a capability is *not* exposed on one or
/// more surfaces. The presence of an exception is what distinguishes an
/// intentional single-surface capability from an accidental parity gap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SingleSurfaceException {
    /// The surface(s) the capability is deliberately absent from.
    pub absent_from: &'static [Surface],
    /// Architecture-decision / story reference justifying the gap.
    pub decision_ref: &'static str,
    /// Human-readable rationale.
    pub rationale: &'static str,
}

/// One v-next capability and its coverage across the three parity surfaces.
#[derive(Debug, Clone, Copy)]
pub struct Capability {
    /// Stable machine identifier, e.g. `visibility`.
    pub id: &'static str,
    /// Human-readable summary.
    pub summary: &'static str,
    /// Per-surface coverage records. A surface may appear at most once.
    pub coverage: &'static [SurfaceCoverage],
    /// `Some` iff this capability deliberately omits one or more surfaces.
    pub exception: Option<SingleSurfaceException>,
}

impl Capability {
    /// `true` iff the capability has a coverage entry for `surface`.
    pub fn covers(&self, surface: Surface) -> bool {
        self.coverage.iter().any(|c| c.surface() == surface)
    }

    /// `true` iff the capability is annotated as deliberately absent from
    /// `surface`.
    pub fn excepted_on(&self, surface: Surface) -> bool {
        self.exception
            .map(|e| e.absent_from.contains(&surface))
            .unwrap_or(false)
    }
}

/// Read-only mirror of the closed MCP tool catalog
/// (`apps/mcp/src/tools/mod.rs::registry()`, pinned by its
/// `registry_is_the_closed_tool_set` test). Kept here so the parity test
/// can cross-check MCP coverage evidence without wire-schema depending on the
/// `apps/mcp` binary. If the app catalog changes, that crate's own test fails
/// first; this mirror then has to be updated, which forces a parity re-review.
pub const CANONICAL_MCP_TOOLS: &[&str] = &[
    "run_prompt",
    "get_visibility",
    "compare_brands",
    "get_citations",
    "list_trends",
    "search_benchmarks",
    "recommend.list",
    "recommend.show",
    "recommend.ack",
    "recommend.dismiss",
    "recommend.mark_acted",
    "audit",
    "list_plugins",
    "install_plugin",
];

/// Read-only mirror of the `/v1` paths declared by the `gen-openapi`
/// `build_spec()` (`crates/wire-schema/src/bin/gen-openapi.rs`). Used to
/// cross-check Web/API coverage evidence whose evidence string is a `/v1` path.
/// `build_spec` lives in a binary (not importable as a lib), so the path set is
/// mirrored here; the binary's own tests pin the same paths.
pub const KNOWN_V1_PATHS: &[&str] = &[
    "/v1/comparisons",
    "/v1/healthz",
    "/v1/runs",
    "/v1/citations/summary",
    "/v1/visibility/trend",
    "/v1/prompt-runs",
    "/v1/setup/status",
    "/v1/setup/clickhouse/install",
    "/v1/setup/clickhouse/install-stream",
    "/v1/recommendations/generate",
    "/v1/recommendations",
    "/v1/recommendations/metrics",
    "/v1/recommendations/{id}",
    "/v1/recommendations/{id}/state",
    "/v1/projects/{project_id}/events",
    "/v1/plugins",
    "/v1/plugins/install",
    "/v1/marketplace/plugins",
];

/// The v-next capability registry: the single source of truth for which
/// surfaces expose which capability.
///
/// Backfill policy (Story 36.10 scope): existing capabilities are recorded with
/// their current coverage; deliberate gaps are annotated. New capabilities must
/// either reach all three surfaces or carry a [`SingleSurfaceException`] — the
/// CI contract test enforces this by construction.
pub const REGISTRY: &[Capability] = &[
    Capability {
        id: "run_prompt",
        summary: "Dispatch a one-shot prompt run across the project's providers.",
        coverage: &[
            SurfaceCoverage::Cli("opengeo run"),
            SurfaceCoverage::WebApi("/v1/prompt-runs"),
            SurfaceCoverage::Mcp("run_prompt"),
        ],
        exception: None,
    },
    Capability {
        id: "visibility",
        summary: "Visibility score trend per prompt over a time window.",
        coverage: &[
            SurfaceCoverage::Cli("opengeo report"),
            SurfaceCoverage::WebApi("/v1/visibility/trend"),
            SurfaceCoverage::Mcp("get_visibility"),
        ],
        exception: None,
    },
    Capability {
        id: "compare_brands",
        summary: "Deterministic brand-vs-competitors comparison matrix.",
        coverage: &[
            SurfaceCoverage::Cli("opengeo report"),
            SurfaceCoverage::WebApi("/v1/comparisons"),
            SurfaceCoverage::Mcp("compare_brands"),
        ],
        exception: None,
    },
    Capability {
        id: "citations",
        summary: "Top cited domains / source types over a window.",
        coverage: &[
            SurfaceCoverage::Cli("opengeo report"),
            SurfaceCoverage::WebApi("/v1/citations/summary"),
            SurfaceCoverage::Mcp("get_citations"),
        ],
        exception: None,
    },
    Capability {
        id: "trends",
        summary: "Significant visibility/citation trend detections.",
        coverage: &[
            SurfaceCoverage::Cli("opengeo report"),
            SurfaceCoverage::WebApi("/v1/visibility/trend"),
            SurfaceCoverage::Mcp("list_trends"),
        ],
        exception: None,
    },
    Capability {
        id: "audit",
        summary: "Crawl owned pages and score citation-readiness.",
        coverage: &[
            SurfaceCoverage::Cli("opengeo audit"),
            SurfaceCoverage::WebApi("/audit"),
            SurfaceCoverage::Mcp("audit"),
        ],
        exception: None,
    },
    Capability {
        id: "recommend_list",
        summary: "List active GEO recommendations for the project.",
        coverage: &[
            SurfaceCoverage::Cli("opengeo recommend list"),
            SurfaceCoverage::WebApi("/v1/recommendations"),
            SurfaceCoverage::Mcp("recommend.list"),
        ],
        exception: None,
    },
    Capability {
        id: "recommend_show",
        summary: "Show one recommendation with full traceability.",
        coverage: &[
            SurfaceCoverage::Cli("opengeo recommend show"),
            SurfaceCoverage::WebApi("/v1/recommendations/{id}"),
            SurfaceCoverage::Mcp("recommend.show"),
        ],
        exception: None,
    },
    Capability {
        id: "recommend_ack",
        summary: "Acknowledge a surfaced recommendation.",
        coverage: &[
            SurfaceCoverage::Cli("opengeo recommend ack"),
            SurfaceCoverage::WebApi("/v1/recommendations/{id}/state"),
            SurfaceCoverage::Mcp("recommend.ack"),
        ],
        exception: None,
    },
    Capability {
        id: "recommend_dismiss",
        summary: "Dismiss a recommendation.",
        coverage: &[
            SurfaceCoverage::Cli("opengeo recommend dismiss"),
            SurfaceCoverage::WebApi("/v1/recommendations/{id}/state"),
            SurfaceCoverage::Mcp("recommend.dismiss"),
        ],
        exception: None,
    },
    Capability {
        id: "recommend_mark_acted",
        summary: "Mark a recommendation as acted, with optional evidence.",
        coverage: &[
            SurfaceCoverage::Cli("opengeo recommend mark-acted"),
            SurfaceCoverage::WebApi("/v1/recommendations/{id}/state"),
            SurfaceCoverage::Mcp("recommend.mark_acted"),
        ],
        exception: None,
    },
    // ---- Annotated single-surface exceptions -------------------------------
    Capability {
        id: "search_benchmarks",
        summary: "Search the public cross-project benchmark dataset (project-less).",
        coverage: &[
            SurfaceCoverage::Cli("opengeo benchmark"),
            SurfaceCoverage::Mcp("search_benchmarks"),
        ],
        exception: Some(SingleSurfaceException {
            absent_from: &[Surface::WebApi],
            decision_ref: "architecture-phase3-mcp-server.md §4 (FR-51 project-less)",
            rationale: "Benchmark search is the deliberately project-less, agent-facing \
                 discovery tool; results link out to the public dashboard rather \
                 than rendering inside a project-scoped Web view. No `/v1` \
                 project-scoped endpoint is exposed for it.",
        }),
    },
    Capability {
        id: "plugin_namespaced_passthrough",
        summary: "Plugin-emitted artifacts (trend kinds, providers) reach users \
             through existing surfaces via the `plugin:<id>:<kind>` namespace.",
        coverage: &[
            // Surfaced verbatim through the existing `list_trends` /
            // `mcp::list_providers` outputs — never as new tools/routes/verbs.
            SurfaceCoverage::Mcp("list_trends"),
        ],
        exception: Some(SingleSurfaceException {
            absent_from: &[Surface::Cli, Surface::WebApi],
            decision_ref: "L3 / AD-Phase3-PluginsCannotRegisterMcpTools (Story 41.6)",
            rationale: "Decision L3: plugins cannot mint new MCP tools, Web surfaces, or \
                 CLI verbs. They reach the user through *existing* surfaces using \
                 the `plugin:<id>:<kind>` namespace (e.g. plugin trend kinds flow \
                 verbatim through `list_trends`). This is the one accepted parity \
                 exception (Story 41.6 parity-honesty).",
        }),
    },
    Capability {
        id: "list_plugins",
        summary: "List currently-installed plugins with version + signature status.",
        coverage: &[
            SurfaceCoverage::Cli("ogeo plugin list"),
            SurfaceCoverage::WebApi("/v1/plugins"),
            SurfaceCoverage::Mcp("list_plugins"),
        ],
        exception: None,
    },
    Capability {
        id: "install_plugin",
        summary: "Install a plugin from the live registry (checksum + signature \
             verified) by id.",
        coverage: &[
            SurfaceCoverage::Cli("ogeo plugin install"),
            SurfaceCoverage::WebApi("/v1/plugins/install"),
            SurfaceCoverage::Mcp("install_plugin"),
        ],
        exception: None,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_coverage_round_trips() {
        let c = SurfaceCoverage::Mcp("get_visibility");
        assert_eq!(c.surface(), Surface::Mcp);
        assert_eq!(c.evidence(), "get_visibility");
    }

    #[test]
    fn covers_and_excepted_are_consistent() {
        let cap = REGISTRY
            .iter()
            .find(|c| c.id == "search_benchmarks")
            .unwrap();
        assert!(cap.covers(Surface::Mcp));
        assert!(!cap.covers(Surface::WebApi));
        assert!(cap.excepted_on(Surface::WebApi));
        assert!(!cap.excepted_on(Surface::Cli));
    }
}
