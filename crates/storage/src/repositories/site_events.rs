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

/// A `(label, count)` aggregate row used by the operator dashboard for top-pages
/// and top-referrers (Story 47.4). `label` is a site-relative path or a bare
/// referrer domain — both non-PII by the 47.1 privacy contract.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct LabeledCount {
    pub label: String,
    pub count: i64,
}

/// A `(day, count)` aggregate row used by the operator dashboard for the
/// sessions-per-day sparkline (Story 47.4).
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct DailyCount {
    pub day: chrono::NaiveDate,
    pub count: i64,
}

/// A `(event_type, count)` aggregate row — funnel step counts (Story 47.4).
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct EventTypeCount {
    pub event_type: String,
    pub count: i64,
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

        // Story 47.4 — dimension rollups for the operator Site Overview panel.
        // Same finalize/monotonic guard as the event rollup above: once a day is
        // complete (strictly before the current UTC date) its dimension counts
        // are frozen so a post-prune recompute from a partial raw subset cannot
        // reduce them. `path` / `referrer` are non-PII (47.1 contract).
        sqlx::query(
            r#"
            INSERT INTO site_page_rollups (path, day, views, finalized, computed_at)
            SELECT
                path,
                (ts AT TIME ZONE 'UTC')::date AS day,
                COUNT(*)                      AS views,
                ((ts AT TIME ZONE 'UTC')::date < (now() AT TIME ZONE 'UTC')::date) AS finalized,
                now()                         AS computed_at
            FROM site_events
            WHERE event_type = 'page_view' AND path IS NOT NULL
            GROUP BY path, (ts AT TIME ZONE 'UTC')::date
            ON CONFLICT (path, day) DO UPDATE SET
                views       = EXCLUDED.views,
                finalized   = EXCLUDED.finalized,
                computed_at = EXCLUDED.computed_at
            WHERE NOT site_page_rollups.finalized
            "#,
        )
        .execute(self.pool)
        .await?;

        sqlx::query(
            r#"
            INSERT INTO site_referrer_rollups (referrer, day, visits, finalized, computed_at)
            SELECT
                referrer,
                (ts AT TIME ZONE 'UTC')::date AS day,
                COUNT(*)                      AS visits,
                ((ts AT TIME ZONE 'UTC')::date < (now() AT TIME ZONE 'UTC')::date) AS finalized,
                now()                         AS computed_at
            FROM site_events
            WHERE event_type = 'page_view' AND referrer IS NOT NULL AND referrer <> ''
            GROUP BY referrer, (ts AT TIME ZONE 'UTC')::date
            ON CONFLICT (referrer, day) DO UPDATE SET
                visits      = EXCLUDED.visits,
                finalized   = EXCLUDED.finalized,
                computed_at = EXCLUDED.computed_at
            WHERE NOT site_referrer_rollups.finalized
            "#,
        )
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Story 47.4 — operator analytics dashboard read queries.
    //
    // All read from the *aggregate* rollup tables (never raw events), so they are
    // privacy-safe by construction and survive the 30-day raw-retention prune.
    // `period_days` is clamped by the caller to the supported {7, 30} window.
    // ─────────────────────────────────────────────────────────────────────────

    /// Unique sessions per day over the trailing `period_days`, oldest day first.
    /// Sums `unique_sessions` across all event types for each day (a session that
    /// fired any event that day is counted once per event type it touched; this
    /// is the best aggregate available without a per-day session dimension, and
    /// `page_view` dominates so it tracks real unique visits closely).
    pub async fn sessions_per_day(
        &self,
        period_days: i64,
    ) -> Result<Vec<DailyCount>, Error> {
        let rows = sqlx::query_as::<_, DailyCount>(
            r#"
            SELECT day, MAX(unique_sessions) AS count
            FROM site_event_rollups
            WHERE event_type = 'page_view'
              AND day >= (now() AT TIME ZONE 'UTC')::date - make_interval(days => $1::int)
            GROUP BY day
            ORDER BY day ASC
            "#,
        )
        .bind(period_days)
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }

    /// Top `limit` pages by total views over the trailing `period_days`.
    pub async fn top_pages(
        &self,
        period_days: i64,
        limit: i64,
    ) -> Result<Vec<LabeledCount>, Error> {
        let rows = sqlx::query_as::<_, LabeledCount>(
            r#"
            SELECT path AS label, SUM(views)::bigint AS count
            FROM site_page_rollups
            WHERE day >= (now() AT TIME ZONE 'UTC')::date - make_interval(days => $1::int)
            GROUP BY path
            ORDER BY count DESC, label ASC
            LIMIT $2
            "#,
        )
        .bind(period_days)
        .bind(limit)
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }

    /// Top `limit` referrer domains by total visits over the trailing window.
    pub async fn top_referrers(
        &self,
        period_days: i64,
        limit: i64,
    ) -> Result<Vec<LabeledCount>, Error> {
        let rows = sqlx::query_as::<_, LabeledCount>(
            r#"
            SELECT referrer AS label, SUM(visits)::bigint AS count
            FROM site_referrer_rollups
            WHERE day >= (now() AT TIME ZONE 'UTC')::date - make_interval(days => $1::int)
            GROUP BY referrer
            ORDER BY count DESC, label ASC
            LIMIT $2
            "#,
        )
        .bind(period_days)
        .bind(limit)
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }

    /// Total `event_count` per event type over the trailing `period_days`.
    /// Backs both funnel panels (contribute + verify) and the badge-embed chart.
    /// Returns one row per event type that has any events in the window.
    pub async fn event_type_totals(
        &self,
        period_days: i64,
    ) -> Result<Vec<EventTypeCount>, Error> {
        let rows = sqlx::query_as::<_, EventTypeCount>(
            r#"
            SELECT event_type, SUM(event_count)::bigint AS count
            FROM site_event_rollups
            WHERE day >= (now() AT TIME ZONE 'UTC')::date - make_interval(days => $1::int)
            GROUP BY event_type
            "#,
        )
        .bind(period_days)
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }

    /// Daily badge-embed serve counts over the trailing `period_days`, oldest
    /// first. Backs the Badge Embeds bar chart.
    pub async fn badge_embeds_per_day(
        &self,
        period_days: i64,
    ) -> Result<Vec<DailyCount>, Error> {
        let rows = sqlx::query_as::<_, DailyCount>(
            r#"
            SELECT day, SUM(event_count)::bigint AS count
            FROM site_event_rollups
            WHERE event_type = 'badge_embed_view'
              AND day >= (now() AT TIME ZONE 'UTC')::date - make_interval(days => $1::int)
            GROUP BY day
            ORDER BY day ASC
            "#,
        )
        .bind(period_days)
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }

    /// Verify funnel counts grouped by method (`dns` | `email`). Method lives in
    /// the raw event `properties`, which the rollup drops — so this is the one
    /// dashboard query that reads raw `site_events`. Still aggregate (GROUP BY),
    /// still no PII (method is a fixed enum, not identity), and bounded to the
    /// 30-day raw-retention window. Returns `(event_type, method, count)` rows.
    pub async fn verify_counts_by_method(
        &self,
        period_days: i64,
    ) -> Result<Vec<(String, String, i64)>, Error> {
        let rows = sqlx::query(
            r#"
            SELECT
                event_type,
                COALESCE(properties->>'method', 'unknown') AS method,
                COUNT(*)::bigint AS count
            FROM site_events
            WHERE event_type IN ('verify_start', 'verify_complete', 'verify_fail')
              AND ts >= now() - make_interval(days => $1::int)
            GROUP BY event_type, method
            "#,
        )
        .bind(period_days)
        .fetch_all(self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                (
                    r.get::<String, _>("event_type"),
                    r.get::<String, _>("method"),
                    r.get::<i64, _>("count"),
                )
            })
            .collect())
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

    /// Story 47.4 — dashboard read queries over the aggregate rollups.
    #[sqlx::test(migrations = "./migrations")]
    async fn dashboard_reads_aggregate_from_rollups(pool: PgPool) {
        let repo = SiteEventRepo::new(&pool);

        // Three page_views across two paths + one referrer; one verify_start(dns)
        // and one verify_complete(dns); two badge embeds. All today.
        let s1 = uuid::Uuid::new_v4();
        let s2 = uuid::Uuid::new_v4();
        repo.insert("page_view", s1, Some("/"), Some("google.com"), &serde_json::json!({}))
            .await
            .unwrap();
        repo.insert("page_view", s1, Some("/leaderboard"), Some("google.com"), &serde_json::json!({}))
            .await
            .unwrap();
        repo.insert("page_view", s2, Some("/"), None, &serde_json::json!({}))
            .await
            .unwrap();
        repo.insert("verify_start", s1, None, None, &serde_json::json!({"method": "dns"}))
            .await
            .unwrap();
        repo.insert("verify_complete", s1, None, None, &serde_json::json!({"method": "dns"}))
            .await
            .unwrap();
        repo.insert("badge_embed_view", s2, None, None, &serde_json::json!({}))
            .await
            .unwrap();
        repo.insert("badge_embed_view", s2, None, None, &serde_json::json!({}))
            .await
            .unwrap();

        repo.compute_rollups().await.unwrap();

        // sessions_per_day: today has 2 unique sessions for page_view.
        let spd = repo.sessions_per_day(7).await.unwrap();
        assert_eq!(spd.len(), 1);
        assert_eq!(spd[0].count, 2);

        // top_pages: "/" has 2 views, "/leaderboard" 1.
        let pages = repo.top_pages(7, 5).await.unwrap();
        assert_eq!(pages[0].label, "/");
        assert_eq!(pages[0].count, 2);
        assert_eq!(pages[1].label, "/leaderboard");
        assert_eq!(pages[1].count, 1);

        // top_referrers: google.com has 2 visits.
        let refs = repo.top_referrers(7, 5).await.unwrap();
        assert_eq!(refs[0].label, "google.com");
        assert_eq!(refs[0].count, 2);

        // event_type_totals includes verify + badge counts.
        let totals = repo.event_type_totals(7).await.unwrap();
        let badge = totals.iter().find(|t| t.event_type == "badge_embed_view").unwrap();
        assert_eq!(badge.count, 2);

        // badge_embeds_per_day: today = 2.
        let badges = repo.badge_embeds_per_day(30).await.unwrap();
        assert_eq!(badges.iter().map(|d| d.count).sum::<i64>(), 2);

        // verify_counts_by_method: dns start=1, complete=1.
        let vm = repo.verify_counts_by_method(30).await.unwrap();
        assert!(vm.contains(&("verify_start".to_string(), "dns".to_string(), 1)));
        assert!(vm.contains(&("verify_complete".to_string(), "dns".to_string(), 1)));
    }

    /// Empty rollups → every dashboard read returns an empty vec (drives the web
    /// empty-state; no crash, no synthetic zeroes).
    #[sqlx::test(migrations = "./migrations")]
    async fn dashboard_reads_are_empty_when_no_data(pool: PgPool) {
        let repo = SiteEventRepo::new(&pool);
        assert!(repo.sessions_per_day(7).await.unwrap().is_empty());
        assert!(repo.top_pages(7, 5).await.unwrap().is_empty());
        assert!(repo.top_referrers(7, 5).await.unwrap().is_empty());
        assert!(repo.event_type_totals(7).await.unwrap().is_empty());
        assert!(repo.badge_embeds_per_day(30).await.unwrap().is_empty());
        assert!(repo.verify_counts_by_method(30).await.unwrap().is_empty());
    }
}
