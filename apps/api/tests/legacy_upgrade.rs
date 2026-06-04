//! Epic 36 Story 36.11 — Legacy single-project upgrade path (RISK-6).
//!
//! GUARANTEE: an existing v0.2.0 **single-project** deployment keeps working
//! unchanged after upgrading to the multi-project runtime. The back-compat
//! code already exists in two places:
//!
//! - **36.2** — per-request resolution with a legacy *sole-active-project*
//!   fallback (`apps/api/src/extractors/project.rs`): a request that sends NO
//!   `X-OpenGEO-Project` header still resolves, as long as exactly one active
//!   project exists.
//! - **36.7** — per-project provider-secret keying with a legacy *global*
//!   read fallback (`crates/core/src/secret_store.rs`): a secret stored under
//!   the bare `<provider>` key (the pre-36.7 shape) still resolves for any
//!   project when no project-scoped key exists.
//!
//! This suite is the **upgrade fixture** that proves the guarantee end to end:
//! it seeds a database in the v0.2.0 single-project *shape* (exactly one
//! project, no siblings), boots the real multi-project resolution path with no
//! header, and asserts the sole project resolves AND its data (a prompt) is
//! reachable through the resolved scope. It then asserts the secret fallback.
//!
//! The DB-backed cases need a live Postgres and are `#[ignore]`d (run with
//! `--ignored`), mirroring `project_header.rs` and the other `*_live_db`
//! suites:
//!
//! ```text
//! DATABASE_URL=postgres://opengeo:opengeo@localhost:5447/opengeo_test \
//!   cargo test -p opengeo-api --test legacy_upgrade -- --ignored
//! ```

use opengeo_api::extractors::resolve_project;
use opengeo_core::{
    get_provider_secret, project_id_for_name, prompt_id_for, set_provider_secret, BrandConfig,
    InMemoryStore, Secret, SecretStore, SecretStoreError,
};
use opengeo_storage::models::PromptRow;
use opengeo_storage::repositories::projects::ProjectRepo;
use opengeo_storage::Storage;
use sqlx::PgPool;

/// These tests manipulate the process-global `projects` table (the
/// sole-active fallback is a COUNT over it), so they must not run concurrently
/// with each other — or with `project_header.rs`. Each uses unique brand names
/// AND resets pre-existing rows, but the sole-active math is only deterministic
/// under serialization. A static async mutex serialises this suite.
static DB_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Connect, migrate (this is the simulated v0.2.0 -> multi-project *upgrade*:
/// the same migrations a real deployment would run), then archive every
/// pre-existing project so the sole-active fallback math is deterministic for
/// this run, isolating us from sibling tests sharing the database.
async fn fresh_storage() -> Option<(Storage, PgPool)> {
    let url = std::env::var("DATABASE_URL").ok()?;
    let pool = PgPool::connect(&url).await.expect("connect");
    let storage = Storage::from_pool(pool.clone());
    storage.migrate().await.expect("migrate");
    sqlx::query("UPDATE projects SET archived_at = now() WHERE archived_at IS NULL")
        .execute(&pool)
        .await
        .expect("reset projects");
    Some((storage, pool))
}

async fn seed_project(pool: &PgPool, name: &str) -> opengeo_core::ProjectId {
    ProjectRepo::new(pool)
        .create_project(&BrandConfig {
            name: name.to_string(),
            variants: Vec::new(),
            site_url: None,
        })
        .await
        .expect("seed project")
}

/// Seed one prompt for `project_id`, returning the prompt name written. This is
/// the project-scoped "data" we later assert is reachable through the resolved
/// sole-active scope (i.e. resolution lands on the project that actually owns
/// the row, not some empty default).
async fn seed_prompt(pool: &PgPool, brand: &str, project_id: opengeo_core::ProjectId) -> String {
    let prompt_name = "legacy-prompt";
    let row = PromptRow {
        id: prompt_id_for(brand, prompt_name),
        project_id,
        name: prompt_name.to_string(),
        text: "what is the best widget?".to_string(),
        tags: Vec::new(),
        organization_id: None,
        tenant_id: None,
        created_at: chrono::Utc::now(),
    };
    Storage::from_pool(pool.clone())
        .prompts()
        .insert(&row)
        .await
        .expect("seed prompt");
    prompt_name.to_string()
}

/// THE upgrade fixture. v0.2.0 single-project shape: exactly one project, with
/// project-scoped data. Booting the multi-project resolver with NO header must
/// resolve to that sole project (legacy behavior preserved) AND its prompt must
/// be reachable through the resolved scope.
#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn legacy_single_project_resolves_without_header() {
    let _guard = DB_LOCK.lock().await;
    let Some((storage, pool)) = fresh_storage().await else {
        return;
    };

    // v0.2.0 shape: one project and nothing else.
    let brand = format!("legacy-{}", uuid::Uuid::new_v4());
    let project_id = seed_project(&pool, &brand).await;
    let prompt_name = seed_prompt(&pool, &brand, project_id).await;

    // Multi-project resolution path, NO explicit value and NO
    // `X-OpenGEO-Project` header — exactly how a pre-upgrade single-project
    // client calls the API. The sole-active fallback (36.2) must resolve it.
    let scope = resolve_project(&storage, None, None)
        .await
        .expect("sole-active fallback must resolve a single-project deployment with no header");
    assert_eq!(
        scope.id(),
        project_id,
        "no-header request must resolve to the one existing project"
    );
    assert_eq!(scope.name(), brand);

    // Its data is reachable THROUGH the resolved scope: read the prompt back
    // scoped to the resolved id. A wrong resolution (e.g. an empty default
    // project) would return zero prompts here.
    let prompts = storage
        .prompts()
        .list_by_project(scope.id())
        .await
        .expect("list prompts for resolved project");
    assert_eq!(
        prompts.len(),
        1,
        "the resolved sole project must own the seeded prompt"
    );
    assert_eq!(prompts[0].name, prompt_name);
    assert_eq!(prompts[0].project_id, project_id);
}

/// The reserved `"default"` sentinel a single-project deployment may hard-code
/// must route through the same sole-active fallback (36.2) post-upgrade.
#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn legacy_default_sentinel_resolves_to_sole_project() {
    let _guard = DB_LOCK.lock().await;
    let Some((storage, pool)) = fresh_storage().await else {
        return;
    };
    let brand = format!("legacy-default-{}", uuid::Uuid::new_v4());
    let project_id = seed_project(&pool, &brand).await;

    let scope = resolve_project(&storage, None, Some("default"))
        .await
        .expect("\"default\" sentinel must resolve to the sole project");
    assert_eq!(scope.id(), project_id);
    assert_eq!(scope.id(), project_id_for_name(&brand));
}

/// Provider-secret back-compat (36.7): a key written under the legacy GLOBAL
/// namespace (bare `<provider>`, the pre-36.7 shape an upgraded deployment
/// already holds) must still resolve for the project via the read fallback when
/// no project-scoped key exists.
#[test]
fn legacy_global_provider_secret_still_resolves_after_upgrade() {
    let store = InMemoryStore::new();

    // Pre-upgrade state: the secret exists ONLY under the bare-provider key.
    store
        .set("openai", Secret::new("sk-v0-2-0-legacy"))
        .expect("seed legacy global secret");

    // Post-upgrade read is project-scoped, but no project-scoped key was ever
    // written — the fallback must surface the legacy global secret.
    let project_id = project_id_for_name("legacy-brand");
    let resolved = get_provider_secret(&store, project_id.to_string().as_str(), "openai")
        .expect("legacy global provider secret must resolve via the back-compat fallback");
    assert_eq!(resolved.expose(), "sk-v0-2-0-legacy");

    // A provider that was never stored anywhere is still a clean NotFound.
    assert!(matches!(
        get_provider_secret(&store, project_id.to_string().as_str(), "anthropic"),
        Err(SecretStoreError::NotFound { .. })
    ));
}

/// Once the operator re-keys a provider secret under the project-scoped
/// namespace, that scoped value must win over any lingering legacy global key —
/// proving the upgrade path lets a deployment migrate off the global key
/// without ambiguity.
#[test]
fn project_scoped_secret_supersedes_legacy_after_rekey() {
    let store = InMemoryStore::new();
    let project_id = project_id_for_name("legacy-brand").to_string();

    // Lingering pre-upgrade global key + a freshly written project-scoped key.
    store
        .set("openai", Secret::new("sk-legacy-global"))
        .unwrap();
    set_provider_secret(
        &store,
        &project_id,
        "openai",
        Secret::new("sk-project-scoped"),
    )
    .unwrap();

    assert_eq!(
        get_provider_secret(&store, &project_id, "openai")
            .unwrap()
            .expose(),
        "sk-project-scoped",
        "the project-scoped key must take precedence over the legacy global one"
    );
}
