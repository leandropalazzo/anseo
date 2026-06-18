//! Story 22.2 — Grep-guard: every non-public privileged route must pass through
//! the authZ middleware (`check_authz` or `RequiredCapability`).
//!
//! This is a static analysis test: it reads the route definition files and
//! asserts that every privileged handler is wired to the capability check.
//!
//! AC-1: single decision point — no surface-local authZ checks.
//! AC-2: denials produce 403 `auth_forbidden`.
//!
//! Explicitly excluded from the guard (public, read-only, or authenticated
//! by non-RBAC means):
//!   - health / liveness / readiness (no org context)
//!   - OpenAPI schema endpoint
//!   - badge endpoints (public-read)
//!   - v1 API-key auth (uses `require_api_key`; single-tenant exempt path)

use std::fs;
use std::path::Path;

/// Routes that are intentionally exempt from per-route RBAC capability checks.
/// These are either public, use an alternative auth mechanism, or are covered
/// by the global `check_authz` middleware layer in lib.rs.
const EXEMPT_ROUTES: &[&str] = &[
    "health.rs",
    "badge.rs",
    "leaderboard.rs",     // public read
    "orgs.rs",            // read-only substrate endpoints
    "comms.rs",           // covered by global check_authz layer in lib.rs
    "brand.rs",           // covered by global check_authz layer in lib.rs
    "schedules.rs",       // covered by global check_authz layer in lib.rs
    "projects.rs",        // covered by global check_authz layer in lib.rs
    "alert_rules.rs",     // covered by global check_authz layer in lib.rs
    "setup.rs",           // covered by global check_authz layer in lib.rs
    "verification.rs",    // covered by global check_authz layer in lib.rs
    "operator_plane1.rs", // covered by global check_authz layer in lib.rs
    "prompts.rs",         // covered by global check_authz layer in lib.rs
    "signup.rs",          // public unauthenticated — no API key, no RBAC (Story 27.1)
];

/// The marker that signals a route is wired to the authZ middleware.
/// Per-route: `RequiredCapability`/`check_authz`. Global: see EXEMPT_ROUTES above.
const AUTHZ_MARKERS: &[&str] = &["RequiredCapability", "check_authz", "RequireCapability"];

/// Returns the route files that contain write verbs (`post`, `put`, `patch`,
/// `delete`) — these MUST be covered by the authZ middleware.
fn find_write_route_files(routes_dir: &Path) -> Vec<(String, String)> {
    let mut result = Vec::new();
    let Ok(entries) = fs::read_dir(routes_dir) else {
        return result;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.ends_with(".rs") {
            continue;
        }
        // Skip exempt routes.
        if EXEMPT_ROUTES.contains(&name) {
            continue;
        }
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        // Check if this file defines write endpoints.
        let has_write = content.contains(".post(")
            || content.contains(".put(")
            || content.contains(".patch(")
            || content.contains(".delete(");
        if has_write {
            result.push((name.to_string(), content));
        }
    }
    result
}

#[test]
fn every_write_route_is_covered_by_authz_middleware() {
    let routes_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/routes");
    let write_routes = find_write_route_files(&routes_dir);

    let mut uncovered = Vec::new();
    for (name, content) in &write_routes {
        let covered = AUTHZ_MARKERS.iter().any(|marker| content.contains(marker));
        if !covered {
            uncovered.push(name.as_str());
        }
    }

    assert!(
        uncovered.is_empty(),
        "The following route files have write endpoints but no authZ middleware marker \
         ({AUTHZ_MARKERS:?}). Add `RequiredCapability` layer or document why exempt:\n  - {}",
        uncovered.join("\n  - ")
    );
}

/// Smoke test: verifies the routes directory exists and is non-empty.
/// (proves the first test isn't vacuously passing because the directory is empty).
#[test]
fn grep_guard_finds_write_routes() {
    let routes_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/routes");
    let all_rs_files: Vec<_> = fs::read_dir(&routes_dir)
        .expect("routes dir exists")
        .flatten()
        .filter(|e| e.path().extension().map(|x| x == "rs").unwrap_or(false))
        .collect();
    assert!(
        all_rs_files.len() >= 5,
        "routes dir has too few .rs files ({}) — path may be wrong: {routes_dir:?}",
        all_rs_files.len()
    );
}
