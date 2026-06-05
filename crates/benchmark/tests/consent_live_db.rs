//! Story 13.1 live-DB test — consent round-trip through the
//! benchmark_consent repo. Gated behind the `live_db_tests` feature on
//! crates/benchmark.

#![cfg(feature = "live_db_tests")]

use anseo_benchmark::TERMS_VERSION;
use anseo_core::ProjectId;
use anseo_storage::Storage;
use uuid::Uuid;

async fn fresh_storage() -> Storage {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL required");
    let s = Storage::connect(&url).await.unwrap();
    s.migrate().await.unwrap();
    s
}

async fn seed_project(s: &Storage) -> ProjectId {
    let pid = ProjectId::new();
    let uuid = Uuid::from_bytes(pid.into_ulid().to_bytes());
    sqlx::query("INSERT INTO projects (id, name) VALUES ($1, $2)")
        .bind(uuid)
        .bind(format!("consent-{}", uuid))
        .execute(s.pool())
        .await
        .expect("seed project");
    pid
}

#[tokio::test]
#[serial_test::serial]
async fn optin_then_latest_returns_optin_row_with_terms_version() {
    let s = fresh_storage().await;
    let pid = seed_project(&s).await;
    s.benchmark_consent()
        .record_optin(pid, TERMS_VERSION, Some("leandro"), Some("ci first run"))
        .await
        .expect("optin");

    let latest = s
        .benchmark_consent()
        .latest_for_project(pid)
        .await
        .expect("query")
        .expect("row present");
    assert_eq!(latest.event, "optin");
    assert_eq!(latest.terms_version, TERMS_VERSION);
    assert_eq!(latest.actor.as_deref(), Some("leandro"));
}

#[tokio::test]
#[serial_test::serial]
async fn optout_after_optin_is_the_new_latest() {
    let s = fresh_storage().await;
    let pid = seed_project(&s).await;
    s.benchmark_consent()
        .record_optin(pid, TERMS_VERSION, None, None)
        .await
        .unwrap();
    s.benchmark_consent()
        .record_optout(pid, TERMS_VERSION, Some("leandro"), Some("opting out"))
        .await
        .unwrap();
    let latest = s
        .benchmark_consent()
        .latest_for_project(pid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(latest.event, "optout");
}

#[tokio::test]
#[serial_test::serial]
async fn latest_returns_none_when_project_never_opted_in() {
    let s = fresh_storage().await;
    let pid = seed_project(&s).await;
    let latest = s.benchmark_consent().latest_for_project(pid).await.unwrap();
    assert!(latest.is_none());
}
