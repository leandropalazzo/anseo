//! Story 19.5 `[rec-7]` — SM-14 adoption metric + plugin-Kind quarantine.
//!
//! The metric counts `Acted ∨ Measured` over `Surfaced ∨ later`, and MUST
//! exclude plugin-emitted Kinds (`plugin_source IS NOT NULL`) from BOTH the
//! numerator and the denominator. This test seeds a project with a mix of
//! first-party and plugin-sourced rows across the lifecycle and asserts the
//! plugin rows never move the number, while still confirming they remain
//! queryable (they surface to users elsewhere; they're just metric-quarantined).
//!
//! Runs against an ephemeral schema via `#[sqlx::test]` (requires DATABASE_URL).

use anseo_storage::repositories::recommendations::{NewRecommendation, RecommendationsRepo};
use sqlx::PgPool;
use uuid::Uuid;

/// Insert a row in a given lifecycle `state`, optionally tagged with a
/// `plugin_source`. `kind`/`fingerprint` are made unique per call so the dedup
/// index never collapses two rows we mean to count separately.
async fn seed(
    repo: &RecommendationsRepo<'_>,
    project_id: Uuid,
    seq: u32,
    state: &str,
    plugin_source: Option<&str>,
) {
    let rec = NewRecommendation {
        id: Uuid::new_v4(),
        project_id,
        kind: format!("kind-{seq}"),
        severity: "medium".to_string(),
        confidence_band: "high".to_string(),
        state: state.to_string(),
        summary: format!("row {seq}"),
        payload: serde_json::json!({}),
        traceability: serde_json::json!({}),
        reproducibility_class: "deterministic".to_string(),
        reproducibility_note: None,
        tags: vec![],
        input_fingerprint: format!("fp-{seq}"),
        engine_version: "0.1.0".to_string(),
        plugin_source: plugin_source.map(str::to_string),
    };
    repo.insert(rec)
        .await
        .unwrap()
        .expect("seed row should insert");
}

#[sqlx::test(migrations = "./migrations")]
async fn rec_7_sm14_excludes_plugin_kinds(pool: PgPool) {
    let repo = RecommendationsRepo::new(&pool);
    let project = Uuid::new_v4();

    // First-party funnel: 5 surfaced-or-later, of which 2 are acted/measured.
    seed(&repo, project, 1, "surfaced", None).await;
    seed(&repo, project, 2, "acknowledged", None).await;
    seed(&repo, project, 3, "acted", None).await;
    seed(&repo, project, 4, "measured", None).await;
    seed(&repo, project, 5, "dismissed", None).await;
    // `generated` never surfaced → excluded from the denominator.
    seed(&repo, project, 6, "generated", None).await;

    // Plugin-sourced rows across the funnel — must NOT move the metric.
    seed(&repo, project, 7, "acted", Some("test.mock-plugin")).await;
    seed(&repo, project, 8, "measured", Some("test.mock-plugin")).await;
    seed(&repo, project, 9, "surfaced", Some("test.mock-plugin")).await;

    let m = repo.sm14_metric(project).await.unwrap();
    assert_eq!(m.numerator, 2, "only first-party acted+measured count");
    assert_eq!(m.denominator, 5, "only first-party surfaced-or-later count");
    assert_eq!(m.rate(), Some(2.0 / 5.0));

    // Quarantine, not deletion: the plugin rows are still in the table and
    // surface to users via the active-rows query (panel + MCP path).
    let active = repo.find_active_by_project(project).await.unwrap();
    let plugin_rows = active.iter().filter(|r| r.plugin_source.is_some()).count();
    assert!(
        plugin_rows >= 2,
        "plugin rows still surface in the list path"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn rec_7_sm14_zero_denominator_is_none(pool: PgPool) {
    let repo = RecommendationsRepo::new(&pool);
    let project = Uuid::new_v4();
    // Only a generated row → nothing has surfaced → rate is n/a.
    seed(&repo, project, 1, "generated", None).await;
    let m = repo.sm14_metric(project).await.unwrap();
    assert_eq!(m.denominator, 0);
    assert_eq!(m.rate(), None);
}
