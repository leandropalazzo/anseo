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
        // reduce them.
        //
        // DEFENSE IN DEPTH (Finding 1): the public ingest endpoint already
        // normalizes `path`/`referrer` at the trust boundary, but pre-existing
        // raw rows (or any future ingest bug) could still carry a full URL, query
        // string, or email-like value. We re-normalize HERE before the value
        // becomes durable in the aggregate so raw PII can never poison the
        // dashboard:
        //   * path  — keep only the site-relative path: strip scheme+host from a
        //             full URL, drop the query string and fragment; anything that
        //             isn't a recognizable path is bucketed to '(other)'.
        //   * referrer — reduce to the bare host: strip scheme/userinfo/path/
        //             query/fragment, lowercase; an `@` (email-like) or a value
        //             with no dot is bucketed to '(other)'.
        sqlx::query(
            r#"
            INSERT INTO site_page_rollups (path, day, views, finalized, computed_at)
            SELECT
                norm_path,
                day,
                COUNT(*)                      AS views,
                (day < (now() AT TIME ZONE 'UTC')::date) AS finalized,
                now()                         AS computed_at
            FROM (
                SELECT
                    (ts AT TIME ZONE 'UTC')::date AS day,
                    CASE
                        -- Reject control chars outright.
                        WHEN path ~ '[\x00-\x1f]' THEN '(other)'
                        -- Full URL → keep the path component only (strip
                        -- scheme+host, then drop BOTH the query string AND the
                        -- fragment so a fragment-carried token
                        -- (`/account#token=abc`) is never made durable.
                        WHEN path ~* '^https?://' THEN
                            COALESCE(
                                NULLIF(
                                    split_part(
                                        split_part(
                                            regexp_replace(path, '^https?://[^/]*', '', 'i'),
                                            '?', 1
                                        ),
                                        '#', 1
                                    ),
                                    ''
                                ),
                                '/'
                            )
                        -- Already a site-relative path → drop query string + fragment.
                        WHEN path LIKE '/%' THEN
                            split_part(split_part(path, '?', 1), '#', 1)
                        ELSE '(other)'
                    END AS norm_path
                FROM site_events
                WHERE event_type = 'page_view' AND path IS NOT NULL AND path <> ''
            ) AS normalized
            WHERE norm_path <> ''
            GROUP BY norm_path, day
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
                norm_ref,
                day,
                COUNT(*)                      AS visits,
                (day < (now() AT TIME ZONE 'UTC')::date) AS finalized,
                now()                         AS computed_at
            FROM (
                SELECT
                    (ts AT TIME ZONE 'UTC')::date AS day,
                    CASE
                        WHEN host ~ '^[a-z0-9.-]+$' AND host LIKE '%.%'
                            THEN host
                        ELSE '(other)'
                    END AS norm_ref
                FROM (
                    SELECT
                        ts,
                        -- Strip scheme, userinfo, port, path/query/fragment →
                        -- bare lowercased host. An email-like value keeps its `@`
                        -- ... no: userinfo strip removes up to the LAST '@', so an
                        -- email 'a@b.com' becomes 'b.com'; guard below still
                        -- accepts only clean hosts, and the trust-boundary
                        -- normalizer already bucketed emails. We additionally drop
                        -- anything with a remaining '@'.
                        lower(
                            split_part(
                                split_part(
                                    regexp_replace(
                                        regexp_replace(referrer, '^(https?:)?//', '', 'i'),
                                        '^[^/@?#]*@', ''
                                    ),
                                    '/', 1
                                ),
                                ':', 1
                            )
                        ) AS host
                    FROM site_events
                    WHERE event_type = 'page_view'
                      AND referrer IS NOT NULL AND referrer <> ''
                      AND referrer !~ '[\x00-\x1f]'
                      AND position('@' in referrer) = 0
                ) AS hosts
            ) AS normalized
            WHERE norm_ref <> ''
            GROUP BY norm_ref, day
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
    //
    // WINDOW SEMANTICS: an N-day window means today + (N-1) previous calendar
    // days. For DATE-bucketed rollup tables that is `day >= current_date -
    // (N-1)`. We subtract one so 7d = 7 calendar days (not 8). The
    // timestamp-backed verify query (`verify_counts_by_method`) projects `ts` to
    // its UTC calendar day and applies the SAME boundary, so all panels cover an
    // identical N-calendar-day window.
    // ─────────────────────────────────────────────────────────────────────────

    /// Unique sessions per day over the trailing `period_days`, oldest day first.
    /// Sums `unique_sessions` across all event types for each day (a session that
    /// fired any event that day is counted once per event type it touched; this
    /// is the best aggregate available without a per-day session dimension, and
    /// `page_view` dominates so it tracks real unique visits closely).
    pub async fn sessions_per_day(&self, period_days: i64) -> Result<Vec<DailyCount>, Error> {
        let rows = sqlx::query_as::<_, DailyCount>(
            r#"
            SELECT day, MAX(unique_sessions) AS count
            FROM site_event_rollups
            WHERE event_type = 'page_view'
              -- N-day window = today + (N-1) previous calendar days. Subtract one
              -- so 7d covers 7 days (not 8), matching the verify query's
              -- UTC-calendar-day window.
              AND day >= (now() AT TIME ZONE 'UTC')::date - make_interval(days => ($1::int - 1))
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
            WHERE day >= (now() AT TIME ZONE 'UTC')::date - make_interval(days => ($1::int - 1))
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
            WHERE day >= (now() AT TIME ZONE 'UTC')::date - make_interval(days => ($1::int - 1))
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
    pub async fn event_type_totals(&self, period_days: i64) -> Result<Vec<EventTypeCount>, Error> {
        let rows = sqlx::query_as::<_, EventTypeCount>(
            r#"
            SELECT event_type, SUM(event_count)::bigint AS count
            FROM site_event_rollups
            WHERE day >= (now() AT TIME ZONE 'UTC')::date - make_interval(days => ($1::int - 1))
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
    pub async fn badge_embeds_per_day(&self, period_days: i64) -> Result<Vec<DailyCount>, Error> {
        let rows = sqlx::query_as::<_, DailyCount>(
            r#"
            SELECT day, SUM(event_count)::bigint AS count
            FROM site_event_rollups
            WHERE event_type = 'badge_embed_view'
              AND day >= (now() AT TIME ZONE 'UTC')::date - make_interval(days => ($1::int - 1))
            GROUP BY day
            ORDER BY day ASC
            "#,
        )
        .bind(period_days)
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }

    /// Verify funnel counts grouped by method (`dns` | `email` | `unknown`).
    /// Method lives in the raw event `properties`, which the rollup drops — so
    /// this is the one dashboard query that reads raw `site_events`. Still
    /// aggregate (GROUP BY), still no PII, and bounded to the 30-day
    /// raw-retention window. Returns `(event_type, method, count)` rows.
    ///
    /// DEFENSE IN DEPTH (Finding 1): `method` rides in the UNAUTHENTICATED
    /// `/v1/site-events` body. The ingest endpoint now buckets it to the closed
    /// enum at the trust boundary, but a pre-existing raw row (ingested before
    /// that fix) could still carry a poisoned arbitrary string. We re-bucket
    /// HERE in SQL — anything outside the closed `{dns, email}` set (incl. the
    /// `dns_txt` / `email_magic_link` canonical spellings, normalized to the
    /// short label) collapses to `unknown` — so a raw value can never surface
    /// verbatim on `/v1/analytics/funnels`.
    pub async fn verify_counts_by_method(
        &self,
        period_days: i64,
    ) -> Result<Vec<(String, String, i64)>, Error> {
        let rows = sqlx::query(
            r#"
            SELECT
                event_type,
                CASE lower(trim(COALESCE(properties->>'method', '')))
                    WHEN 'dns'              THEN 'dns'
                    WHEN 'dns_txt'          THEN 'dns'
                    WHEN 'email'            THEN 'email'
                    WHEN 'email_magic_link' THEN 'email'
                    ELSE 'unknown'
                END AS method,
                COUNT(*)::bigint AS count
            FROM site_events
            WHERE event_type IN ('verify_start', 'verify_complete', 'verify_fail')
              -- Same UTC CALENDAR-day window the rollup-backed queries use, so the
              -- Verify Funnel covers exactly N calendar days and agrees with the
              -- contribute funnel + overview for the same period (rather than a
              -- rolling now()-N timestamp window that bleeds into the (N+1)th day).
              AND (ts AT TIME ZONE 'UTC')::date >= (now() AT TIME ZONE 'UTC')::date - make_interval(days => ($1::int - 1))
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
        repo.insert(
            "page_view",
            s1,
            Some("/"),
            Some("google.com"),
            &serde_json::json!({}),
        )
        .await
        .unwrap();
        repo.insert(
            "page_view",
            s1,
            Some("/leaderboard"),
            Some("google.com"),
            &serde_json::json!({}),
        )
        .await
        .unwrap();
        repo.insert("page_view", s2, Some("/"), None, &serde_json::json!({}))
            .await
            .unwrap();
        repo.insert(
            "verify_start",
            s1,
            None,
            None,
            &serde_json::json!({"method": "dns"}),
        )
        .await
        .unwrap();
        repo.insert(
            "verify_complete",
            s1,
            None,
            None,
            &serde_json::json!({"method": "dns"}),
        )
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
        let badge = totals
            .iter()
            .find(|t| t.event_type == "badge_embed_view")
            .unwrap();
        assert_eq!(badge.count, 2);

        // badge_embeds_per_day: today = 2.
        let badges = repo.badge_embeds_per_day(30).await.unwrap();
        assert_eq!(badges.iter().map(|d| d.count).sum::<i64>(), 2);

        // verify_counts_by_method: dns start=1, complete=1.
        let vm = repo.verify_counts_by_method(30).await.unwrap();
        assert!(vm.contains(&("verify_start".to_string(), "dns".to_string(), 1)));
        assert!(vm.contains(&("verify_complete".to_string(), "dns".to_string(), 1)));
    }

    /// Finding 2 — N-day window size. An N-day window must cover exactly N
    /// calendar days (today + N-1 previous), NOT N+1. We seed page_view rollups
    /// on a span of distinct days and assert the 7d read returns exactly the 7
    /// days ending today — the day at offset -7 (the 8th) must be excluded.
    #[sqlx::test(migrations = "./migrations")]
    async fn dashboard_window_covers_exactly_n_days(pool: PgPool) {
        let repo = SiteEventRepo::new(&pool);

        // Insert one page_view + one verify_start per day for offsets 0..=8 days
        // ago (9 distinct days). page_view gets a unique session so
        // unique_sessions = 1/day; verify_start backs the cross-check that the
        // raw timestamp-backed verify query shares the rollup's calendar window.
        // We anchor each row to NOON UTC of its offset day so the rolling
        // `now() - N days` boundary (which would land mid-afternoon) and the
        // calendar-day boundary disagree on the edge day if the windows differ.
        for offset in 0..=8 {
            let sid = uuid::Uuid::new_v4();
            repo.insert("page_view", sid, Some("/"), None, &serde_json::json!({}))
                .await
                .unwrap();
            let vsid = uuid::Uuid::new_v4();
            repo.insert(
                "verify_start",
                vsid,
                None,
                None,
                &serde_json::json!({"method": "dns"}),
            )
            .await
            .unwrap();
            sqlx::query(
                "UPDATE site_events SET ts = \
                 ((now() AT TIME ZONE 'UTC')::date - make_interval(days => $1::int) \
                  + interval '12 hours') \
                 WHERE session_id = ANY($2)",
            )
            .bind(offset)
            .bind(&[sid, vsid][..])
            .execute(&pool)
            .await
            .unwrap();
        }

        repo.compute_rollups().await.unwrap();

        // 7-day window = today + 6 previous = exactly 7 distinct days. The day at
        // offset -7 and -8 must be excluded (NOT 8 days).
        let spd = repo.sessions_per_day(7).await.unwrap();
        assert_eq!(
            spd.len(),
            7,
            "7d window must cover exactly 7 calendar days, not 8 (off-by-one)"
        );

        // top_pages over the same window: "/" appears on each of the 7 in-window
        // days → 7 views (not 8).
        let pages = repo.top_pages(7, 5).await.unwrap();
        assert_eq!(pages[0].label, "/");
        assert_eq!(pages[0].count, 7, "7d window over-counted (off-by-one)");

        // 30-day window covers all 9 seeded days.
        assert_eq!(repo.sessions_per_day(30).await.unwrap().len(), 9);

        // The raw timestamp-backed verify query must share the rollup's
        // UTC-calendar-day window EXACTLY: 7d → 7 verify_start rows (today + 6
        // previous), never 8. With rows anchored at noon UTC, a rolling
        // `now() - 7 days` window would have admitted the offset-7 day's
        // afternoon row; the calendar-day boundary excludes it.
        let vm7: i64 = repo
            .verify_counts_by_method(7)
            .await
            .unwrap()
            .iter()
            .filter(|(et, _, _)| et == "verify_start")
            .map(|(_, _, c)| c)
            .sum();
        assert_eq!(
            vm7, 7,
            "verify window must match the rollup window: exactly 7 calendar days, not 8"
        );
        // 30d covers all 9 seeded verify_start rows — agrees with sessions_per_day(30).
        let vm30: i64 = repo
            .verify_counts_by_method(30)
            .await
            .unwrap()
            .iter()
            .filter(|(et, _, _)| et == "verify_start")
            .map(|(_, _, c)| c)
            .sum();
        assert_eq!(vm30, 9);
    }

    /// Finding 1 (defense in depth) — even if raw `site_events` rows carry a full
    /// URL with a query string or an email-like referrer (e.g. a pre-existing row
    /// or a future ingest bug), the rollup must NOT make them durable raw: the
    /// page is reduced to its site-relative path and the email-like referrer is
    /// bucketed to the safe sentinel — never surfaced raw on the dashboard.
    #[sqlx::test(migrations = "./migrations")]
    async fn rollup_normalizes_raw_url_and_email_referrer(pool: PgPool) {
        let repo = SiteEventRepo::new(&pool);

        // A poisoned raw row: full URL path with a query string + email referrer.
        repo.insert(
            "page_view",
            uuid::Uuid::new_v4(),
            Some("https://evil.example.com/account?token=abc123&email=jane@corp.com"),
            Some("jane.doe@corp.com"),
            &serde_json::json!({}),
        )
        .await
        .unwrap();
        // A clean baseline row.
        repo.insert(
            "page_view",
            uuid::Uuid::new_v4(),
            Some("/leaderboard"),
            Some("https://google.com/search?q=x"),
            &serde_json::json!({}),
        )
        .await
        .unwrap();
        // Finding 2: a fragment-carried token on a FULL URL must be stripped by
        // the SQL normalizer, not rolled up as `/settings#token=secret`.
        repo.insert(
            "page_view",
            uuid::Uuid::new_v4(),
            Some("https://example.com/settings#token=secret"),
            None,
            &serde_json::json!({}),
        )
        .await
        .unwrap();
        // Finding 1: an UPPERCASE-scheme full URL is accepted by the `~*`
        // (case-insensitive) predicate, so the scheme+host strip MUST also be
        // case-insensitive — otherwise `HTTPS://example.com/account?token=abc`
        // is rolled up (and shown on the dashboard) as the raw full URL.
        repo.insert(
            "page_view",
            uuid::Uuid::new_v4(),
            Some("HTTPS://example.com/account?token=abc"),
            None,
            &serde_json::json!({}),
        )
        .await
        .unwrap();

        repo.compute_rollups().await.unwrap();

        // top_pages: the full URL is reduced to "/account" (no scheme, host, or
        // query string); the raw URL must NOT appear.
        let pages = repo.top_pages(7, 10).await.unwrap();
        let labels: Vec<&str> = pages.iter().map(|p| p.label.as_str()).collect();
        assert!(
            labels.contains(&"/account"),
            "full URL must be reduced to its site-relative path; got {labels:?}"
        );
        assert!(labels.contains(&"/leaderboard"));
        assert!(
            !labels
                .iter()
                .any(|l| l.contains("evil.example.com") || l.contains("token=")),
            "no raw URL / query string may be durable: {labels:?}"
        );
        // Finding 1: the UPPERCASE-scheme URL must reduce to "/account" — the
        // scheme+host strip is case-insensitive, so neither the raw `HTTPS://...`
        // nor the `?token=abc` query may be durable.
        assert!(
            labels.contains(&"/account"),
            "UPPERCASE-scheme full URL must reduce to its path; got {labels:?}"
        );
        assert!(
            !labels
                .iter()
                .any(|l| l.to_ascii_lowercase().contains("https://")),
            "no raw (upper- or lower-case scheme) URL may be durable: {labels:?}"
        );
        // Finding 2: the fragment-carried token URL rolls up as bare "/settings"
        // — the `#token=secret` fragment must be stripped, never durable.
        assert!(
            labels.contains(&"/settings"),
            "full URL with fragment must reduce to its path; got {labels:?}"
        );
        assert!(
            !labels
                .iter()
                .any(|l| l.contains('#') || l.contains("secret")),
            "no fragment / fragment-carried token may be durable: {labels:?}"
        );

        // top_referrers: the email-like referrer must be dropped/bucketed — never
        // stored raw (no '@', no raw email value). The clean one is reduced to its
        // bare domain.
        let refs = repo.top_referrers(7, 10).await.unwrap();
        let rlabels: Vec<&str> = refs.iter().map(|r| r.label.as_str()).collect();
        assert!(
            !rlabels
                .iter()
                .any(|l| l.contains('@') || l.contains("jane")),
            "no email-like referrer may be stored raw: {rlabels:?}"
        );
        assert!(
            rlabels.contains(&"google.com"),
            "clean referrer must reduce to bare domain: {rlabels:?}"
        );
    }

    /// Finding 1 (defense in depth) — a poisoned `method` in raw `properties`
    /// (e.g. an email submitted via the unauthenticated ingest before the
    /// trust-boundary fix) must NEVER be surfaced verbatim by
    /// `verify_counts_by_method`: it is bucketed to the closed `{dns, email,
    /// unknown}` enum, with the canonical `dns_txt`/`email_magic_link` spellings
    /// normalized to the short label.
    #[sqlx::test(migrations = "./migrations")]
    async fn verify_method_is_bucketed_to_closed_enum(pool: PgPool) {
        let repo = SiteEventRepo::new(&pool);

        // A poisoned raw row: method is an email string.
        repo.insert(
            "verify_start",
            uuid::Uuid::new_v4(),
            None,
            None,
            &serde_json::json!({"method": "jane@example.com"}),
        )
        .await
        .unwrap();
        // Another poisoned row: a long garbage string.
        repo.insert(
            "verify_start",
            uuid::Uuid::new_v4(),
            None,
            None,
            &serde_json::json!({"method": "x".repeat(4096)}),
        )
        .await
        .unwrap();
        // Canonical long spelling → short label.
        repo.insert(
            "verify_complete",
            uuid::Uuid::new_v4(),
            None,
            None,
            &serde_json::json!({"method": "dns_txt"}),
        )
        .await
        .unwrap();
        repo.insert(
            "verify_complete",
            uuid::Uuid::new_v4(),
            None,
            None,
            &serde_json::json!({"method": "email_magic_link"}),
        )
        .await
        .unwrap();

        let vm = repo.verify_counts_by_method(30).await.unwrap();
        let methods: std::collections::HashSet<&str> =
            vm.iter().map(|(_, m, _)| m.as_str()).collect();

        // Only closed-enum labels are ever returned.
        for m in &methods {
            assert!(
                matches!(*m, "dns" | "email" | "unknown"),
                "method outside closed enum surfaced verbatim: {m:?}"
            );
        }
        // The poisoned email / garbage are never present verbatim.
        assert!(
            !vm.iter()
                .any(|(_, m, _)| m.contains('@') || m.contains("jane") || m.len() > 16),
            "raw poisoned method leaked: {vm:?}"
        );
        // Two poisoned verify_start rows → unknown=2.
        assert!(vm.contains(&("verify_start".to_string(), "unknown".to_string(), 2)));
        // Canonical spellings normalized to short labels.
        assert!(vm.contains(&("verify_complete".to_string(), "dns".to_string(), 1)));
        assert!(vm.contains(&("verify_complete".to_string(), "email".to_string(), 1)));
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
