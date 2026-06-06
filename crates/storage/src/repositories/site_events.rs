//! Site-events repository — Story 47.1 (public site analytics).
//!
//! Backs the unauthenticated `POST /v1/site-events` ingest endpoint plus the
//! nightly rollup + 30-day raw-retention worker job.
//!
//! # Privacy invariants (architecture A2)
//!
//! * **No IP is ever written here.** Rate-limiting happens at the Axum edge with
//!   an ephemeral in-memory key; this repo never sees or stores an IP.
//! * `session_id` is an ephemeral per-visit UUID, not linked to identity.
//! * `referrer` is a domain only, never a full URL (the caller normalizes).
//!
//! Dynamic sqlx only (`sqlx::query` / `query_as::<_, Row>`) — no `query!`
//! macros (project HARD RULE).

use sqlx::PgPool;
use sqlx::Row as _;

use crate::error::Error;

/// The fixed event taxonomy (architecture A3). Ten named events covering the
/// full public funnel. The ingest endpoint validates against this list and
/// silently drops anything else (204) so a client can't enumerate the allowlist
/// through error probing.
pub const SITE_EVENT_TYPES: &[&str] = &[
    "page_view",
    "leaderboard_view",
    "brand_profile_view",
    "contribute_start",
    "contribute_step",
    "contribute_complete",
    "verify_start",
    "verify_complete",
    "verify_fail",
    "badge_embed_view",
];

/// Returns `true` if `event_type` is in the fixed taxonomy.
pub fn is_known_event_type(event_type: &str) -> bool {
    SITE_EVENT_TYPES.contains(&event_type)
}

/// A single aggregate rollup row, grouped by `(event_type, day)`.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct SiteEventRollup {
    pub event_type: String,
    pub day: chrono::NaiveDate,
    pub event_count: i64,
    pub unique_sessions: i64,
    /// `true` once the day is complete (past midnight UTC) and the rollup is
    /// frozen — a later prune-driven recompute cannot reduce it.
    pub finalized: bool,
    pub computed_at: chrono::DateTime<chrono::Utc>,
}

/// Borrowing repository over a shared pool.
pub struct SiteEventRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> SiteEventRepo<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Insert one raw site event. The caller MUST have already validated
    /// `event_type` against [`is_known_event_type`]; unknown types should be
    /// silently dropped before reaching here. No IP is accepted or stored.
    pub async fn insert(
        &self,
        event_type: &str,
        session_id: uuid::Uuid,
        path: Option<&str>,
        referrer: Option<&str>,
        properties: &serde_json::Value,
    ) -> Result<(), Error> {
        sqlx::query(
            r#"
            INSERT INTO site_events (event_type, session_id, path, referrer, properties)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(event_type)
        .bind(session_id)
        .bind(path)
        .bind(referrer)
        .bind(properties)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Nightly rollup: aggregate raw `site_events` into `site_event_rollups`,
    /// grouped by `(event_type, date)`.
    ///
    /// **Durable / monotonic (Story 47.1 fix).** A rollup row is `finalized` the
    /// moment its day is complete — i.e. the day is strictly before the current
    /// UTC date, so no more raw rows can land in it. Once finalized, a later
    /// recompute can never overwrite it. This is what makes the rollup safe in
    /// the face of the 30-day raw-retention prune: after retention starts
    /// deleting a day's raw rows, recomputing that day from the surviving subset
    /// would yield a smaller, partial count — the `WHERE NOT ... finalized`
    /// guard rejects that clobber. Today's (still-open) row keeps updating until
    /// it too rolls over past midnight and gets frozen.
    ///
    /// Idempotent — re-running upserts the same buckets (an interrupted/retried
    /// job is safe). Returns the number of `(event_type, day)` buckets written.
    pub async fn compute_rollups(&self) -> Result<u64, Error> {
        let result = sqlx::query(
            r#"
            INSERT INTO site_event_rollups
                (event_type, day, event_count, unique_sessions, finalized, computed_at)
            SELECT
                event_type,
                (ts AT TIME ZONE 'UTC')::date           AS day,
                COUNT(*)                                AS event_count,
                COUNT(DISTINCT session_id)              AS unique_sessions,
                -- A day is complete (and thus finalized) once it is strictly
                -- before the current UTC date: no further raw rows can arrive.
                ((ts AT TIME ZONE 'UTC')::date < (now() AT TIME ZONE 'UTC')::date) AS finalized,
                now()                                   AS computed_at
            FROM site_events
            GROUP BY event_type, (ts AT TIME ZONE 'UTC')::date
            ON CONFLICT (event_type, day) DO UPDATE SET
                event_count     = EXCLUDED.event_count,
                unique_sessions = EXCLUDED.unique_sessions,
                finalized       = EXCLUDED.finalized,
                computed_at     = EXCLUDED.computed_at
            -- Never overwrite an already-finalized (complete) day. A prune-driven
            -- recompute from a partial set of surviving raw rows cannot reduce a
            -- frozen count.
            WHERE NOT site_event_rollups.finalized
            "#,
        )
        .execute(self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// Retention: delete raw `site_events` rows older than `retention_days`.
    /// The rollup table is intentionally untouched (aggregate-only survives, per
    /// A2). Returns the number of raw rows deleted.
    pub async fn prune_raw_older_than(&self, retention_days: i64) -> Result<u64, Error> {
        let result = sqlx::query(
            r#"
            DELETE FROM site_events
            WHERE ts < now() - make_interval(days => $1::int)
            "#,
        )
        .bind(retention_days)
        .execute(self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// Fetch all rollup rows (test/dashboard helper), most-recent day first.
    pub async fn list_rollups(&self) -> Result<Vec<SiteEventRollup>, Error> {
        let rows = sqlx::query_as::<_, SiteEventRollup>(
            r#"
            SELECT event_type, day, event_count, unique_sessions, finalized, computed_at
            FROM site_event_rollups
            ORDER BY day DESC, event_type ASC
            "#,
        )
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }

    /// Count raw events (test helper).
    pub async fn count_raw(&self) -> Result<i64, Error> {
        let row = sqlx::query("SELECT COUNT(*) AS n FROM site_events")
            .fetch_one(self.pool)
            .await?;
        Ok(row.get::<i64, _>("n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taxonomy_has_ten_events_and_validates() {
        assert_eq!(SITE_EVENT_TYPES.len(), 10);
        assert!(is_known_event_type("page_view"));
        assert!(is_known_event_type("badge_embed_view"));
        assert!(!is_known_event_type("totally_made_up"));
        assert!(!is_known_event_type(""));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn insert_and_rollup_and_retention(pool: PgPool) {
        let repo = SiteEventRepo::new(&pool);

        // Insert two events for the same session/type today.
        let sid = uuid::Uuid::new_v4();
        repo.insert(
            "page_view",
            sid,
            Some("/"),
            Some("google.com"),
            &serde_json::json!({}),
        )
        .await
        .unwrap();
        repo.insert(
            "page_view",
            sid,
            Some("/leaderboard"),
            None,
            &serde_json::json!({"path": "/leaderboard"}),
        )
        .await
        .unwrap();
        // A different event type, different session.
        repo.insert(
            "verify_start",
            uuid::Uuid::new_v4(),
            None,
            None,
            &serde_json::json!({"method": "dns"}),
        )
        .await
        .unwrap();

        assert_eq!(repo.count_raw().await.unwrap(), 3);

        // Rollup groups by (event_type, day): page_view => 2 events / 1 session,
        // verify_start => 1 event / 1 session.
        repo.compute_rollups().await.unwrap();
        let rollups = repo.list_rollups().await.unwrap();
        let page_view = rollups
            .iter()
            .find(|r| r.event_type == "page_view")
            .expect("page_view rollup");
        assert_eq!(page_view.event_count, 2);
        assert_eq!(page_view.unique_sessions, 1);
        let verify = rollups
            .iter()
            .find(|r| r.event_type == "verify_start")
            .expect("verify_start rollup");
        assert_eq!(verify.event_count, 1);

        // No IP column exists — the privacy contract. A SELECT on `ip` must fail.
        let ip_probe = sqlx::query("SELECT ip FROM site_events LIMIT 1")
            .fetch_optional(&pool)
            .await;
        assert!(ip_probe.is_err(), "site_events must NOT have an ip column");

        // Retention with 0 days deletes today's raw rows but leaves rollups.
        let deleted = repo.prune_raw_older_than(0).await.unwrap();
        assert_eq!(deleted, 3);
        assert_eq!(repo.count_raw().await.unwrap(), 0);
        // Rollup table untouched by retention.
        assert!(!repo.list_rollups().await.unwrap().is_empty());
    }

    /// Regression for the rollup-undercount finding: once a day is complete and
    /// its rollup is finalized, a later recompute — after the retention prune has
    /// removed some of that day's raw rows — must NOT clobber the frozen count
    /// with a smaller, partial one. Rollups are durable/monotonic.
    #[sqlx::test(migrations = "./migrations")]
    async fn finalized_rollup_is_not_reduced_by_recompute_after_prune(pool: PgPool) {
        let repo = SiteEventRepo::new(&pool);

        // Three events for the same type, all dated *two days ago* (a complete,
        // past UTC day) so the first rollup will finalize them.
        for _ in 0..3 {
            repo.insert(
                "page_view",
                uuid::Uuid::new_v4(),
                Some("/"),
                None,
                &serde_json::json!({}),
            )
            .await
            .unwrap();
        }
        sqlx::query("UPDATE site_events SET ts = now() - interval '2 days'")
            .execute(&pool)
            .await
            .unwrap();

        // First rollup: day is in the past → row is finalized with the full count.
        repo.compute_rollups().await.unwrap();
        let first = repo
            .list_rollups()
            .await
            .unwrap()
            .into_iter()
            .find(|r| r.event_type == "page_view")
            .expect("page_view rollup");
        assert_eq!(first.event_count, 3);
        assert_eq!(first.unique_sessions, 3);
        assert!(first.finalized, "a complete past day must be finalized");

        // Retention prunes most of that day's raw rows (simulating mid-window
        // pruning). Only a partial subset of the day's raw events survives.
        sqlx::query("DELETE FROM site_events WHERE id IN (SELECT id FROM site_events LIMIT 2)")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(repo.count_raw().await.unwrap(), 1);

        // Re-run the rollup. A naive recompute would write event_count = 1 from
        // the single surviving raw row. The finalize guard must reject it.
        repo.compute_rollups().await.unwrap();
        let after = repo
            .list_rollups()
            .await
            .unwrap()
            .into_iter()
            .find(|r| r.event_type == "page_view")
            .expect("page_view rollup");
        assert_eq!(
            after.event_count, 3,
            "finalized rollup must not be reduced by a post-prune recompute"
        );
        assert_eq!(after.unique_sessions, 3);
        assert!(after.finalized);
    }
}
