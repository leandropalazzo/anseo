//! Story 19.2 `[rec-4]` — dedup invariant. Generating the same fixture twice
//! and inserting every Recommendation both times yields a single *active* row
//! per `(project_id, kind, input_fingerprint)`, enforced by the
//! `recommendations_active_dedup_idx` unique partial index.
//!
//! Runs against an ephemeral schema via `#[sqlx::test]` (storage-crate
//! convention; requires DATABASE_URL).

use opengeo_recommendations::{Engine, Recommendation};
use opengeo_storage::repositories::recommendations::{NewRecommendation, RecommendationsRepo};
use serde_json::Value as JsonValue;
use sqlx::{PgPool, Row};
use uuid::Uuid;

fn ulid_to_uuid(u: ulid::Ulid) -> Uuid {
    Uuid::from_u128(u.0)
}

/// snake_case serde string for a wire enum (Severity/ConfidenceBand/...).
fn enum_str<T: serde::Serialize>(v: &T) -> String {
    match serde_json::to_value(v).unwrap() {
        JsonValue::String(s) => s,
        other => panic!("expected string enum, got {other}"),
    }
}

fn to_new(rec: &Recommendation) -> NewRecommendation {
    NewRecommendation {
        id: ulid_to_uuid(rec.id),
        project_id: ulid_to_uuid(rec.project_id),
        kind: rec.kind.as_str().to_string(),
        severity: enum_str(&rec.severity),
        confidence_band: enum_str(&rec.confidence_band),
        // Wire LifecycleState::New maps to the DB 'generated' state.
        state: "generated".to_string(),
        summary: rec.summary.clone(),
        payload: rec.payload.clone(),
        traceability: serde_json::to_value(&rec.traceability).unwrap(),
        reproducibility_class: enum_str(&rec.reproducibility.class),
        reproducibility_note: rec.reproducibility.note.clone(),
        tags: rec.tags.clone(),
        input_fingerprint: rec.traceability.input_fingerprint.clone(),
        engine_version: rec.engine_version.clone(),
        plugin_source: None,
    }
}

mod fixture {
    include!("../../recommendations/tests/fixture/mod.rs");
}

#[sqlx::test(migrations = "./migrations")]
async fn rec_4_dedup_single_active_row_per_fingerprint(pool: PgPool) {
    let recs = Engine::default().generate(&fixture::full_fixture());
    assert!(!recs.is_empty(), "fixture must produce recommendations");

    let repo = RecommendationsRepo::new(&pool);

    // First pass: every Recommendation inserts.
    let mut first_inserts = 0usize;
    for r in &recs {
        if repo.insert(to_new(r)).await.unwrap().is_some() {
            first_inserts += 1;
        }
    }
    assert_eq!(
        first_inserts,
        recs.len(),
        "every recommendation should insert on the first pass"
    );

    // Second pass: same fixture → every insert is deduped (returns None).
    for r in &recs {
        let inserted = repo.insert(to_new(r)).await.unwrap();
        assert!(
            inserted.is_none(),
            "re-inserting `{}` must be deduped to None",
            r.kind.as_str()
        );
    }

    // Exactly one active row per (project_id, kind, input_fingerprint).
    let dup_groups: i64 = sqlx::query(
        r#"
        SELECT COUNT(*) AS n FROM (
            SELECT project_id, kind, input_fingerprint
            FROM recommendations
            WHERE state NOT IN ('dismissed','measured','stale')
            GROUP BY project_id, kind, input_fingerprint
            HAVING COUNT(*) > 1
        ) dups
        "#,
    )
    .fetch_one(&pool)
    .await
    .unwrap()
    .get("n");
    assert_eq!(
        dup_groups, 0,
        "no active fingerprint may appear more than once"
    );

    let total: i64 = sqlx::query("SELECT COUNT(*) AS n FROM recommendations")
        .fetch_one(&pool)
        .await
        .unwrap()
        .get("n");
    assert_eq!(
        total as usize,
        recs.len(),
        "table should hold exactly one row per generated recommendation"
    );
}
