//! Multi-project storage foundation (Epic 36 / Story 36.1).
//!
//! Storage now permits any number of coexisting projects. These tests pin the
//! registry operations the rest of Epic 36 builds on: multi-project
//! coexistence, per-table `project_id` isolation (no cross-project leakage),
//! the legacy sole-project precedence path, and create/archive.
//!
//! Acceptance criteria coverage:
//! - AC-1: `multiple_projects_coexist` — no single-project rejection with ≥2 rows.
//! - AC-2: `multiple_projects_coexist` — `list_projects` returns all non-archived.
//! - AC-3: `multiple_projects_coexist` — `create_project` derives `project_id` from name.
//! - AC-4: `archive_removes_from_registry_but_preserves_row` — archived excluded from list; data kept.
//! - AC-5: `single_brand_resolves_only_for_the_sole_project` — legacy path resolves sole project.
//! - AC-6: `project_id_scoping_prevents_cross_project_leakage` — no cross-project row leakage.

use anseo_core::{prompt_id_for, BrandConfig, ProjectId};
use anseo_storage::models::PromptRow;
use anseo_storage::Storage;
use chrono::Utc;
use sqlx::PgPool;

fn brand(name: &str) -> BrandConfig {
    BrandConfig {
        name: name.into(),
        variants: vec![format!("{name} Inc")],
        site_url: Some(format!("https://{name}.example")),
    }
}

fn prompt(project_id: ProjectId, brand_name: &str, prompt_name: &str) -> PromptRow {
    PromptRow {
        id: prompt_id_for(brand_name, prompt_name),
        project_id,
        name: prompt_name.into(),
        text: "Who makes the best widget?".into(),
        tags: Vec::new(),
        organization_id: None,
        tenant_id: None,
        created_at: Utc::now(),
    }
}

/// Two projects coexist: storage no longer rejects a >1 project DB, and the
/// registry listing returns both with their derived identities and config.
#[sqlx::test(migrations = "./migrations")]
async fn multiple_projects_coexist(pool: PgPool) {
    let storage = Storage::from_pool(pool);

    let acme = storage
        .projects()
        .create_project(&brand("acme"))
        .await
        .unwrap();
    let globex = storage
        .projects()
        .create_project(&brand("globex"))
        .await
        .unwrap();
    assert_ne!(acme, globex);

    let projects = storage.projects().list_projects().await.unwrap();
    assert_eq!(projects.len(), 2);

    // Each created project is fetchable on its own and carries its config.
    let acme_row = storage.projects().get_project(acme).await.unwrap().unwrap();
    assert_eq!(acme_row.name, "acme");
    let acme_brand = storage.projects().get_brand(acme).await.unwrap().unwrap();
    assert_eq!(acme_brand.variants, vec!["acme Inc".to_string()]);
    assert_eq!(acme_brand.site_url.as_deref(), Some("https://acme.example"));
}

/// Per-table `WHERE project_id = $1` scoping: a child row under one project is
/// never visible through another project's id.
#[sqlx::test(migrations = "./migrations")]
async fn project_id_scoping_prevents_cross_project_leakage(pool: PgPool) {
    let storage = Storage::from_pool(pool);

    let acme = storage
        .projects()
        .create_project(&brand("acme"))
        .await
        .unwrap();
    let globex = storage
        .projects()
        .create_project(&brand("globex"))
        .await
        .unwrap();

    storage
        .prompts()
        .insert(&prompt(acme, "acme", "headline"))
        .await
        .unwrap();
    storage
        .prompts()
        .insert(&prompt(globex, "globex", "tagline"))
        .await
        .unwrap();

    let acme_prompts = storage.prompts().list_by_project(acme).await.unwrap();
    assert_eq!(acme_prompts.len(), 1);
    assert_eq!(acme_prompts[0].project_id, acme);
    assert_eq!(acme_prompts[0].name, "headline");

    let globex_prompts = storage.prompts().list_by_project(globex).await.unwrap();
    assert_eq!(globex_prompts.len(), 1);
    assert_eq!(globex_prompts[0].project_id, globex);
    assert_eq!(globex_prompts[0].name, "tagline");

    // No leakage: acme's prompt never surfaces under globex and vice versa.
    assert!(acme_prompts.iter().all(|p| p.project_id == acme));
    assert!(globex_prompts.iter().all(|p| p.project_id == globex));
}

/// Legacy precedence fallback: `get_single_brand` resolves the sole project,
/// and yields `None` once a second project coexists.
#[sqlx::test(migrations = "./migrations")]
async fn single_brand_resolves_only_for_the_sole_project(pool: PgPool) {
    let storage = Storage::from_pool(pool);

    let acme = storage
        .projects()
        .create_project(&brand("acme"))
        .await
        .unwrap();
    let single = storage
        .projects()
        .get_single_brand()
        .await
        .unwrap()
        .unwrap();
    assert_eq!(single.id, acme);

    storage
        .projects()
        .create_project(&brand("globex"))
        .await
        .unwrap();
    assert!(storage
        .projects()
        .get_single_brand()
        .await
        .unwrap()
        .is_none());
}

/// Archiving drops a project from the active registry and from
/// `get_single_brand`, while leaving the row (and its config) fetchable by id.
/// Archiving a sibling re-exposes a now-sole active project to the legacy path.
#[sqlx::test(migrations = "./migrations")]
async fn archive_removes_from_registry_but_preserves_row(pool: PgPool) {
    let storage = Storage::from_pool(pool);

    let acme = storage
        .projects()
        .create_project(&brand("acme"))
        .await
        .unwrap();
    let globex = storage
        .projects()
        .create_project(&brand("globex"))
        .await
        .unwrap();

    storage.projects().archive_project(globex).await.unwrap();

    let active = storage.projects().list_projects().await.unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, acme);

    // Archived row still fetchable by id (data preserved).
    assert!(storage
        .projects()
        .get_project(globex)
        .await
        .unwrap()
        .is_some());

    // With one active project remaining, the legacy path resolves it again.
    let single = storage
        .projects()
        .get_single_brand()
        .await
        .unwrap()
        .unwrap();
    assert_eq!(single.id, acme);

    // Idempotent re-archive.
    storage.projects().archive_project(globex).await.unwrap();
    assert_eq!(storage.projects().list_projects().await.unwrap().len(), 1);
}
