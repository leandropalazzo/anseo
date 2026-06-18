//! Story 24.1 — org entitlement repository tests.

use anseo_storage::Storage;
use sqlx::PgPool;

#[sqlx::test(migrations = "./migrations")]
async fn upsert_inserts_and_get_returns_entitlement(pool: PgPool) {
    let storage = Storage::from_pool(pool.clone());
    let org_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO organizations (slug, name) VALUES ('billing-org', 'Billing Org') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("insert org");

    storage
        .org_entitlements()
        .upsert(org_id, "pro", 7, Some("cus_123"), Some("sub_123"))
        .await
        .expect("upsert entitlement");

    let row = storage
        .org_entitlements()
        .get(org_id)
        .await
        .expect("get entitlement")
        .expect("row exists");

    assert_eq!(row.org_id, org_id);
    assert_eq!(row.plan, "pro");
    assert_eq!(row.seat_count, 7);
    assert_eq!(row.stripe_customer_id.as_deref(), Some("cus_123"));
}

#[sqlx::test(migrations = "./migrations")]
async fn upsert_updates_existing_org_row(pool: PgPool) {
    let storage = Storage::from_pool(pool.clone());
    let org_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO organizations (slug, name) VALUES ('update-billing-org', 'Update Billing Org') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("insert org");

    storage
        .org_entitlements()
        .upsert(org_id, "pro", 3, Some("cus_456"), Some("sub_old"))
        .await
        .expect("first upsert");
    storage
        .org_entitlements()
        .upsert(org_id, "enterprise", 25, Some("cus_456"), Some("sub_new"))
        .await
        .expect("second upsert");

    let row_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*)::bigint FROM org_entitlements WHERE org_id = $1")
            .bind(org_id)
            .fetch_one(&pool)
            .await
            .expect("count rows");
    let row = storage
        .org_entitlements()
        .get(org_id)
        .await
        .expect("get entitlement")
        .expect("row exists");

    assert_eq!(row_count, 1);
    assert_eq!(row.plan, "enterprise");
    assert_eq!(row.seat_count, 25);
}

#[sqlx::test(migrations = "./migrations")]
async fn get_returns_none_when_missing(pool: PgPool) {
    let storage = Storage::from_pool(pool);

    let row = storage
        .org_entitlements()
        .get(uuid::Uuid::new_v4())
        .await
        .expect("get entitlement");

    assert!(row.is_none());
}
