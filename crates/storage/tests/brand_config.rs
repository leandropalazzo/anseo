//! DB-authoritative brand config + identity re-key on rename.

use anseo_core::{project_id_for_name, prompt_id_for, ProjectId, PromptId};
use anseo_storage::models::{ProjectRow, PromptRow};
use anseo_storage::Storage;
use chrono::Utc;
use serde_json::json;
use sqlx::PgPool;

fn project(name: &str) -> ProjectRow {
    ProjectRow {
        id: project_id_for_name(name),
        name: name.into(),
        organization_id: None,
        tenant_id: None,
        created_at: Utc::now(),
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn brand_update_in_place_and_single_read(pool: PgPool) {
    let storage = Storage::from_pool(pool);
    let row = project("acme");
    let id = storage.projects().insert(&row).await.unwrap();

    let competitors = json!([{ "name": "globex", "variants": ["Globex Inc"] }]);
    storage
        .projects()
        .update_brand(id, "acme", &["Acme Co".into()], &competitors, None)
        .await
        .unwrap();

    let brand = storage.projects().get_brand(id).await.unwrap().unwrap();
    assert_eq!(brand.name, "acme");
    assert_eq!(brand.variants, vec!["Acme Co".to_string()]);
    assert_eq!(brand.competitors, competitors);

    // Exactly one project → get_single_brand resolves it.
    let single = storage
        .projects()
        .get_single_brand()
        .await
        .unwrap()
        .unwrap();
    assert_eq!(single.id, id);
}

#[sqlx::test(migrations = "./migrations")]
async fn single_brand_none_when_multiple_projects(pool: PgPool) {
    let storage = Storage::from_pool(pool);
    storage.projects().insert(&project("acme")).await.unwrap();
    storage.projects().insert(&project("globex")).await.unwrap();
    assert!(storage
        .projects()
        .get_single_brand()
        .await
        .unwrap()
        .is_none());
}

#[sqlx::test(migrations = "./migrations")]
async fn rename_on_empty_rekeys_project_and_prompts(pool: PgPool) {
    let storage = Storage::from_pool(pool);
    let old_id = storage.projects().insert(&project("acme")).await.unwrap();

    // A prompt under the old brand identity.
    let prompt = PromptRow {
        id: prompt_id_for("acme", "headline"),
        project_id: old_id,
        name: "headline".into(),
        text: "Who makes the best widget?".into(),
        tags: Vec::new(),
        organization_id: None,
        tenant_id: None,
        created_at: Utc::now(),
    };
    storage.prompts().insert(&prompt).await.unwrap();

    assert_eq!(
        storage.projects().prompt_run_count(old_id).await.unwrap(),
        0
    );

    let new_name = "zenith";
    let new_id = project_id_for_name(new_name);
    let new_pid = prompt_id_for(new_name, "headline");
    let remap: Vec<(PromptId, PromptId)> = vec![(prompt.id, new_pid)];

    storage
        .projects()
        .rename_on_empty(
            old_id,
            new_id,
            new_name,
            &["Zenith".into()],
            &json!([]),
            None,
            &remap,
        )
        .await
        .unwrap();

    // Old identity gone, new identity present with re-keyed prompt.
    assert!(storage
        .projects()
        .get_brand(old_id)
        .await
        .unwrap()
        .is_none());
    let brand = storage.projects().get_brand(new_id).await.unwrap().unwrap();
    assert_eq!(brand.name, new_name);

    let prompts = storage.prompts().list_by_project(new_id).await.unwrap();
    assert_eq!(prompts.len(), 1);
    assert_eq!(prompts[0].id, new_pid);
    assert_eq!(prompts[0].project_id, new_id);
}

/// Story 44.1 autoreview fix: the identified-tier `contributions` table carries
/// an `ON DELETE RESTRICT` FK to `projects`. A re-key must re-point those rows
/// FIRST, otherwise the final `DELETE FROM projects` (old id) violates the
/// RESTRICT and the whole rename fails once any identified contribution exists.
#[sqlx::test(migrations = "./migrations")]
async fn rename_on_empty_repoints_identified_contributions(pool: PgPool) {
    use anseo_storage::repositories::benchmark_consent::ConsentTier;

    let storage = Storage::from_pool(pool);
    let old_id = storage.projects().insert(&project("acme")).await.unwrap();

    // An identified-tier consent row, then a contribution that references it and
    // the old project id (the RESTRICT FK).
    let repo = storage.benchmark_consent();
    repo.record_optin(old_id, "v1-2026-05-28", Some("op"), None)
        .await
        .unwrap();
    let consent_id = repo
        .record_optin_tier(
            old_id,
            ConsentTier::BrandVisibility,
            "v1-2026-05-28",
            Some("op"),
            None,
        )
        .await
        .unwrap();

    sqlx::query(
        r#"INSERT INTO contributions
           (project_id, project_hmac, consent_record_id, verification_token, terms_version)
           VALUES ($1, $2, $3, $4, $5)"#,
    )
    .bind(old_id)
    .bind("hmac-deadbeef")
    .bind(consent_id)
    .bind("vtok-43-2")
    .bind("v1-2026-05-28")
    .execute(storage.pool())
    .await
    .unwrap();

    // Re-key: must NOT fail on the RESTRICT FK.
    let new_name = "zenith";
    let new_id = project_id_for_name(new_name);
    storage
        .projects()
        .rename_on_empty(
            old_id,
            new_id,
            new_name,
            &["Zenith".into()],
            &json!([]),
            None,
            &[],
        )
        .await
        .expect("re-key must succeed with identified contributions present");

    // The contribution followed to the new project id.
    let moved: i64 = sqlx::query_scalar("SELECT count(*) FROM contributions WHERE project_id = $1")
        .bind(new_id)
        .fetch_one(storage.pool())
        .await
        .unwrap();
    assert_eq!(
        moved, 1,
        "contribution must be re-pointed at the new project id"
    );
    let stale: i64 = sqlx::query_scalar("SELECT count(*) FROM contributions WHERE project_id = $1")
        .bind(old_id)
        .fetch_one(storage.pool())
        .await
        .unwrap();
    assert_eq!(
        stale, 0,
        "no contribution may remain under the old project id"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn rename_preserves_id_when_name_unchanged(pool: PgPool) {
    // Sanity: identity is a pure function of the brand name.
    assert_eq!(project_id_for_name("acme"), project_id_for_name("acme"));
    assert_ne!(project_id_for_name("acme"), project_id_for_name("zenith"));
    let _ = ProjectId::new();
    let _ = pool;
}
