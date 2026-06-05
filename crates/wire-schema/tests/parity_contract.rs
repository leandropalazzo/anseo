//! Parity-as-CI-contract enforcement (Story 36.10, RISK-4 — AC-2).
//!
//! These tests are the *mechanism* behind parity: they fail CI if a registered
//! capability lacks CLI/Web/MCP coverage without an annotated exception, and
//! cross-check the registry against the surfaces wire-schema can cheaply
//! enumerate from Rust (the MCP tool catalog mirror and the OpenAPI path set
//! mirror). See `anseo_wire_schema::parity` for what is and is not machine-
//! asserted, and why.

use std::collections::HashSet;

use anseo_wire_schema::parity::{
    Capability, Surface, SurfaceCoverage, CANONICAL_MCP_TOOLS, KNOWN_V1_PATHS, REGISTRY,
};

/// AC-2 (the core contract): every registered capability is either covered on a
/// surface or carries an annotation marking that surface as a deliberate gap.
/// A capability that is neither covered nor excepted on some surface is an
/// accidental parity gap and fails CI.
#[test]
fn every_capability_is_covered_or_excepted_on_all_three_surfaces() {
    let mut gaps: Vec<String> = Vec::new();
    for cap in REGISTRY {
        for surface in Surface::ALL {
            let covered = cap.covers(surface);
            let excepted = cap.excepted_on(surface);
            if !covered && !excepted {
                gaps.push(format!(
                    "capability `{}` is not exposed on {surface:?} and has no \
                     single_surface_exception covering {surface:?}",
                    cap.id
                ));
            }
            // A surface cannot be simultaneously covered AND annotated absent —
            // that is a contradictory record.
            if covered && excepted {
                gaps.push(format!(
                    "capability `{}` is both covered on {surface:?} and listed in \
                     its exception's `absent_from` — contradictory record",
                    cap.id
                ));
            }
        }
    }
    assert!(
        gaps.is_empty(),
        "parity contract violated:\n  - {}",
        gaps.join("\n  - ")
    );
}

/// Internal consistency: ids are unique, every coverage list is non-empty and
/// surface-unique, and every exception is well-formed (non-empty `absent_from`,
/// non-empty refs).
#[test]
fn registry_is_internally_consistent() {
    let mut ids = HashSet::new();
    for cap in REGISTRY {
        assert!(
            ids.insert(cap.id),
            "duplicate capability id `{}` in REGISTRY",
            cap.id
        );
        assert!(
            !cap.summary.trim().is_empty(),
            "capability `{}` has an empty summary",
            cap.id
        );
        assert!(
            !cap.coverage.is_empty(),
            "capability `{}` has no coverage on any surface",
            cap.id
        );

        let mut seen = HashSet::new();
        for cov in cap.coverage {
            assert!(
                seen.insert(cov.surface()),
                "capability `{}` lists {:?} more than once",
                cap.id,
                cov.surface()
            );
            assert!(
                !cov.evidence().trim().is_empty(),
                "capability `{}` has empty evidence for {:?}",
                cap.id,
                cov.surface()
            );
        }

        if let Some(exc) = cap.exception {
            assert!(
                !exc.absent_from.is_empty(),
                "capability `{}` carries an exception with an empty `absent_from`",
                cap.id
            );
            assert!(
                !exc.decision_ref.trim().is_empty(),
                "capability `{}` exception has an empty decision_ref",
                cap.id
            );
            assert!(
                !exc.rationale.trim().is_empty(),
                "capability `{}` exception has an empty rationale",
                cap.id
            );
        }
    }
}

/// AC-3: the plugin namespaced-MCP pass-through (decision L3 / Story 41.6) is a
/// recognized annotated exception — present, absent from CLI + Web/API, and
/// reaching the user through an *existing* MCP tool rather than a new one.
#[test]
fn plugin_passthrough_is_a_recognized_exception() {
    let cap = REGISTRY
        .iter()
        .find(|c| c.id == "plugin_namespaced_passthrough")
        .expect("plugin pass-through capability must be registered (AC-3)");

    let exc = cap
        .exception
        .expect("plugin pass-through must carry a single_surface_exception (AC-3)");
    assert!(exc.absent_from.contains(&Surface::Cli));
    assert!(exc.absent_from.contains(&Surface::WebApi));
    assert!(
        exc.decision_ref.contains("L3"),
        "exception must reference decision L3"
    );

    // It is reached through an existing MCP tool, never a plugin-minted one.
    assert!(cap.covers(Surface::Mcp));
    if let Some(SurfaceCoverage::Mcp(tool)) =
        cap.coverage.iter().find(|c| c.surface() == Surface::Mcp)
    {
        assert!(
            CANONICAL_MCP_TOOLS.contains(tool),
            "plugin pass-through must reach users via an existing MCP tool, got `{tool}`"
        );
    }
}

/// Cross-check #1: every MCP coverage evidence string names a tool that
/// actually exists in the canonical MCP catalog. Catches a typo'd or stale tool
/// reference in the registry.
#[test]
fn mcp_coverage_evidence_matches_canonical_catalog() {
    for cap in REGISTRY {
        for cov in cap.coverage {
            if let SurfaceCoverage::Mcp(tool) = cov {
                assert!(
                    CANONICAL_MCP_TOOLS.contains(tool),
                    "capability `{}` references MCP tool `{tool}` which is not in \
                     CANONICAL_MCP_TOOLS (mirror of apps/mcp registry)",
                    cap.id
                );
            }
        }
    }
}

/// Cross-check #2: every Web/API coverage evidence that is a `/v1` path must be
/// a path the OpenAPI spec actually declares. Web *routes* (non-`/v1`, e.g.
/// `/audit`) are intentionally skipped — they are Next.js routes asserted by the
/// web e2e suite, not by this Rust test (documented in `parity.rs`).
#[test]
fn v1_webapi_evidence_matches_openapi_paths() {
    for cap in REGISTRY {
        for cov in cap.coverage {
            if let SurfaceCoverage::WebApi(path) = cov {
                if path.starts_with("/v1/") {
                    assert!(
                        KNOWN_V1_PATHS.contains(path),
                        "capability `{}` references `/v1` path `{path}` which is not \
                         in KNOWN_V1_PATHS (mirror of gen-openapi build_spec)",
                        cap.id
                    );
                }
            }
        }
    }
}

/// Coverage floor: the six FR-46..FR-51 MCP tools and the five `recommend.*`
/// tools are the core agent-facing capabilities. Each must appear as MCP
/// coverage *somewhere* in the registry, so a dropped registry entry for a
/// shipped tool is caught. (`audit` is covered by its own capability row.)
#[test]
fn all_canonical_mcp_tools_are_represented_in_the_registry() {
    let covered_tools: HashSet<&str> = REGISTRY
        .iter()
        .flat_map(|c: &Capability| c.coverage.iter())
        .filter_map(|cov| match cov {
            SurfaceCoverage::Mcp(t) => Some(*t),
            _ => None,
        })
        .collect();

    let missing: Vec<&&str> = CANONICAL_MCP_TOOLS
        .iter()
        .filter(|t| !covered_tools.contains(**t))
        .collect();

    assert!(
        missing.is_empty(),
        "these canonical MCP tools have no capability registry entry: {missing:?}"
    );
}
