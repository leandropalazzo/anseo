//! Entity registry repository — Story 43.1 / 43.3.
//!
//! The entity registry is the canonical domain → display-name mapping used by
//! the leaderboard and the claim/verify flow. Every domain stored here is
//! **normalized** (lowercase, `www.` stripped, trailing slash stripped) before
//! any DB operation.
//!
//! # Dedup contract (Story 43.3)
//!
//! False-merge is worse than false-split (a false merge bleeds one brand's
//! badge into another — a defamation vector). The default posture is:
//!
//! * High-confidence exact/near-exact match → **auto-merge** (same normalized
//!   domain resolves to the existing entity).
//! * Ambiguous match (above threshold but below auto-merge confidence) →
//!   **placed in `dedup_review_queue`** with `pending_review` status; the
//!   candidate is NOT merged automatically.
//! * Homoglyph / unicode-confusable detection is part of normalization —
//!   Cyrillic `А` and Latin `A` are folded to the same canonical form.
//!
//! Simultaneous-claim conflicts are handled by the `conflict` transition in
//! [`EntityRepo::mark_pending_conflict`].

use sqlx::PgPool;
use sqlx::Row as _;

use crate::error::Error;

/// Public-facing entity record returned by the repository.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct EntityRecord {
    pub domain: String,
    pub display_name: String,
    pub role: String,
    pub claim_status: String,
    pub verified_at: Option<chrono::DateTime<chrono::Utc>>,
    pub verification_method: Option<String>,
    pub grace_period_start: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Filters for the operator entity-list query (Story 48.4). All present
/// filters AND together; absent filters do not constrain. `domain` is a
/// case-insensitive substring match.
#[derive(Debug, Clone, Default)]
pub struct EntityListFilters {
    pub claim_status: Option<String>,
    pub verification_method: Option<String>,
    /// Case-insensitive substring of the domain.
    pub domain: Option<String>,
}

/// Row counts deleted by [`EntityRepo::erase`] (Story 48.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EraseCounts {
    pub entity_rows: u64,
    pub attempt_rows: u64,
    pub dispute_rows: u64,
}

/// Result of resolving an entity (domain) to its owning project for the
/// crypto-shred decision (Story 48.4). Entities are domain-keyed; KEKs are
/// project-keyed. We can only crypto-shred when a domain maps to EXACTLY ONE
/// project via the identified-contribution linkage (`contributions.entity_domain`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntityProjectMapping {
    /// Exactly one project owns identified contributions for this domain — a
    /// KEK destroy is unambiguous and safe.
    Unique(String),
    /// No identified contribution links this domain to any project — there is
    /// no KEK to destroy for it.
    None,
    /// Two or more distinct projects have identified contributions for this
    /// domain. Destroying any one KEK could shred unrelated contributors, so we
    /// refuse and surface the ambiguity instead.
    Ambiguous { project_ids: Vec<String> },
}

/// A pending dedup review queue entry.
#[derive(Debug, Clone)]
pub struct DedupQueueEntry {
    pub id: uuid::Uuid,
    pub canonical_domain: String,
    pub candidate_domain: String,
    pub candidate_name: String,
    pub similarity_score: i16,
    pub match_reason: String,
    pub status: String,
}

pub struct EntityRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> EntityRepo<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    // -------------------------------------------------------------------------
    // Domain normalization (Story 43.3 AC-1)
    // -------------------------------------------------------------------------

    /// Normalize a raw domain string:
    ///   1. Strip scheme (`https://`, `http://`)
    ///   2. Strip path — keep only hostname
    ///   3. Lowercase
    ///   4. Strip leading `www.`
    ///   5. Strip trailing `.` (DNS absolute form) and `/`
    ///   6. Apply unicode confusable folding (homoglyph → ASCII equivalent)
    pub fn normalize_domain(raw: &str) -> String {
        let s = raw.trim();
        // Strip scheme
        let s = s
            .trim_start_matches("https://")
            .trim_start_matches("http://");
        // Strip path (keep only hostname)
        let s = s.split('/').next().unwrap_or(s);
        // Lowercase
        let s = s.to_lowercase();
        // Strip leading www.
        let s = s.strip_prefix("www.").unwrap_or(&s).to_string();
        // Strip trailing dot (DNS absolute form)
        let s = s.trim_end_matches('.').to_string();
        // Unicode confusable folding
        fold_confusables(&s)
    }

    // -------------------------------------------------------------------------
    // CRUD
    // -------------------------------------------------------------------------

    /// Upsert-on-conflict: insert the entity if the domain is new; if the
    /// domain already exists, return the existing record. Callers inspect the
    /// returned record to detect collisions (display_name differs → merge
    /// suggestion; story 43.3 AC-1 boundary).
    ///
    /// Returns `(record, was_inserted)`.
    pub async fn upsert(
        &self,
        domain: &str,
        display_name: &str,
        role: &str,
    ) -> Result<(EntityRecord, bool), Error> {
        // Try insert first.
        let inserted = sqlx::query(
            r#"
            INSERT INTO entities (domain, display_name, role)
            VALUES ($1, $2, $3)
            ON CONFLICT (domain) DO NOTHING
            "#,
        )
        .bind(domain)
        .bind(display_name)
        .bind(role)
        .execute(self.pool)
        .await?;

        let was_inserted = inserted.rows_affected() > 0;
        let record = self.get(domain).await?.ok_or(Error::NotFound)?;
        Ok((record, was_inserted))
    }

    /// Fetch an entity by normalized domain. Returns `None` if not found.
    pub async fn get(&self, domain: &str) -> Result<Option<EntityRecord>, Error> {
        let row = sqlx::query_as::<_, EntityRecord>(
            r#"
            SELECT domain, display_name, role, claim_status,
                   verified_at, verification_method, grace_period_start,
                   created_at, updated_at
            FROM entities
            WHERE domain = $1
            "#,
        )
        .bind(domain)
        .fetch_optional(self.pool)
        .await?;
        Ok(row)
    }

    /// Operator entity list (Story 48.4). Filters AND together; `domain` is a
    /// case-insensitive substring. Ordered by `created_at DESC` (newest claims
    /// first) for a stable, deterministic page.
    ///
    /// PAGINATION CHOICE: limit/offset. The operator surface is low-volume
    /// (claimed brands number in the hundreds, not millions) and the console
    /// needs jump-to-page UX, for which offset paging is the simplest correct
    /// fit. Cursor paging would buy stability under high churn we do not have
    /// here; if the registry ever grows past offset paging's comfort zone, the
    /// filter shape stays the same and only the page token changes.
    pub async fn list(
        &self,
        filters: &EntityListFilters,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<EntityRecord>, Error> {
        // Dynamic WHERE built with positional binds (no string interpolation of
        // user input — substring is bound, not concatenated).
        let mut sql = String::from(
            "SELECT domain, display_name, role, claim_status, \
                    verified_at, verification_method, grace_period_start, \
                    created_at, updated_at \
             FROM entities WHERE 1=1",
        );
        let mut n = 0;
        if filters.claim_status.is_some() {
            n += 1;
            sql.push_str(&format!(" AND claim_status = ${n}"));
        }
        if filters.verification_method.is_some() {
            n += 1;
            sql.push_str(&format!(" AND verification_method = ${n}"));
        }
        if filters.domain.is_some() {
            n += 1;
            // ILIKE with the bound value wrapped in %…% — case-insensitive
            // substring. The literal is escaped at bind time.
            sql.push_str(&format!(" AND domain ILIKE ${n}"));
        }
        n += 1;
        sql.push_str(&format!(" ORDER BY created_at DESC LIMIT ${n}"));
        n += 1;
        sql.push_str(&format!(" OFFSET ${n}"));

        let mut q = sqlx::query_as::<_, EntityRecord>(&sql);
        if let Some(cs) = &filters.claim_status {
            q = q.bind(cs);
        }
        if let Some(vm) = &filters.verification_method {
            q = q.bind(vm);
        }
        if let Some(d) = &filters.domain {
            q = q.bind(format!("%{}%", d));
        }
        q = q.bind(limit).bind(offset);
        Ok(q.fetch_all(self.pool).await?)
    }

    /// Resolve a domain to its owning project for the crypto-shred decision
    /// (Story 48.4 erase). Looks at the identified-contribution linkage
    /// (`contributions.entity_domain`, migration 20260606140000): a domain maps
    /// unambiguously to a project IFF exactly one distinct `project_id` has
    /// identified contributions for it. Returns [`EntityProjectMapping`].
    ///
    /// This is the ONLY mapping we trust for KEK destruction. Anything else
    /// (zero, or two-or-more projects) is reported so the caller refuses to
    /// guess and never shreds a KEK shared by unrelated contributors.
    pub async fn project_for_domain(&self, domain: &str) -> Result<EntityProjectMapping, Error> {
        let rows = sqlx::query(
            r#"
            SELECT DISTINCT project_id::text AS project_id
            FROM contributions
            WHERE entity_domain = $1
            "#,
        )
        .bind(domain)
        .fetch_all(self.pool)
        .await?;
        let ids: Vec<String> = rows
            .into_iter()
            .map(|r| r.get::<String, _>("project_id"))
            .collect();
        Ok(match ids.len() {
            0 => EntityProjectMapping::None,
            1 => EntityProjectMapping::Unique(ids.into_iter().next().unwrap()),
            _ => EntityProjectMapping::Ambiguous { project_ids: ids },
        })
    }

    /// GDPR erase (Story 48.4): transactionally delete the entity row, its
    /// `verification_attempts`, and identifiable dispute rows for the domain.
    ///
    /// `verification_attempts` and `disputes` are NOT FK-children of `entities`
    /// (both reference the domain as free text, not via a constraint), so we
    /// delete them explicitly in one transaction. `contributions.entity_domain`
    /// is `ON DELETE RESTRICT`, so any live identified contribution would block
    /// the entity delete — that is intentional: the contribution payload is
    /// erased by KEK crypto-shred, not by row deletion, and we must not orphan a
    /// referenced registry row. The caller resolves the crypto-shred separately.
    ///
    /// Returns the number of (entity, attempt, dispute) rows deleted.
    pub async fn erase(&self, domain: &str) -> Result<EraseCounts, Error> {
        let mut tx = self.pool.begin().await?;

        let disputes = sqlx::query(r#"DELETE FROM disputes WHERE domain = $1"#)
            .bind(domain)
            .execute(&mut *tx)
            .await?
            .rows_affected();

        let attempts = sqlx::query(r#"DELETE FROM verification_attempts WHERE domain = $1"#)
            .bind(domain)
            .execute(&mut *tx)
            .await?
            .rows_affected();

        let entity = sqlx::query(r#"DELETE FROM entities WHERE domain = $1"#)
            .bind(domain)
            .execute(&mut *tx)
            .await?
            .rows_affected();

        tx.commit().await?;
        Ok(EraseCounts {
            entity_rows: entity,
            attempt_rows: attempts,
            dispute_rows: disputes,
        })
    }

    /// Update claim status. Transitions:
    ///   `pending` → `verified` / `failed` / `pending_conflict`
    ///   `verified` → `revoked`
    ///   `revoked`  → `unclaimed` (after grace period)
    pub async fn set_claim_status(
        &self,
        domain: &str,
        status: &str,
        verification_method: Option<&str>,
    ) -> Result<(), Error> {
        let now = chrono::Utc::now();
        let verified_at: Option<chrono::DateTime<chrono::Utc>> = if status == "verified" {
            Some(now)
        } else {
            None
        };
        sqlx::query(
            r#"
            UPDATE entities
            SET claim_status        = $2,
                verification_method = COALESCE($3, verification_method),
                verified_at         = CASE WHEN $2 = 'verified' THEN $4 ELSE verified_at END,
                updated_at          = now()
            WHERE domain = $1
            "#,
        )
        .bind(domain)
        .bind(status)
        .bind(verification_method)
        .bind(verified_at)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Begin the 14-day grace period when a verified domain's TXT record is
    /// removed. Sets `claim_status = revoked` and records `grace_period_start`.
    /// Badge is suppressed while `grace_period_start IS NOT NULL` (Story 43.3 AC-5).
    pub async fn set_grace_period_start(&self, domain: &str) -> Result<(), Error> {
        sqlx::query(
            r#"
            UPDATE entities
            SET grace_period_start = now(),
                claim_status       = 'revoked',
                updated_at         = now()
            WHERE domain = $1
              AND claim_status = 'verified'
            "#,
        )
        .bind(domain)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Re-open domains whose 14-day grace period has elapsed — make them
    /// claimable again. Returns the count of domains reopened.
    pub async fn reopen_after_grace_period(&self) -> Result<u64, Error> {
        let res = sqlx::query(
            r#"
            UPDATE entities
            SET claim_status        = 'unclaimed',
                grace_period_start  = NULL,
                verified_at         = NULL,
                verification_method = NULL,
                updated_at          = now()
            WHERE claim_status = 'revoked'
              AND grace_period_start IS NOT NULL
              AND grace_period_start < now() - INTERVAL '14 days'
            "#,
        )
        .execute(self.pool)
        .await?;
        Ok(res.rows_affected())
    }

    /// Transition a domain to `pending_conflict` when a second claimant
    /// simultaneously asserts the same already-pending domain (Story 43.3 AC-4).
    /// The first claimant retains `verified` status if already verified.
    pub async fn mark_pending_conflict(&self, domain: &str) -> Result<(), Error> {
        sqlx::query(
            r#"
            UPDATE entities
            SET claim_status = 'pending_conflict',
                updated_at   = now()
            WHERE domain = $1
              AND claim_status = 'pending'
            "#,
        )
        .bind(domain)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    // -------------------------------------------------------------------------
    // Dedup review queue (Story 43.3 AC-2 / AC-3)
    // -------------------------------------------------------------------------

    /// Enqueue an ambiguous near-duplicate for human review (Story 43.3 AC-2).
    /// Only called when `score` is in the review-queue band: [REVIEW_QUEUE_THRESHOLD, AUTO_MERGE_THRESHOLD).
    pub async fn enqueue_dedup_review(
        &self,
        canonical_domain: &str,
        candidate_domain: &str,
        candidate_name: &str,
        similarity_score: i16,
        match_reason: &str,
    ) -> Result<uuid::Uuid, Error> {
        let id = uuid::Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO dedup_review_queue
                (id, canonical_domain, candidate_domain, candidate_name,
                 similarity_score, match_reason)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(id)
        .bind(canonical_domain)
        .bind(candidate_domain)
        .bind(candidate_name)
        .bind(similarity_score)
        .bind(match_reason)
        .execute(self.pool)
        .await?;
        Ok(id)
    }

    /// Operator adjudication: approve merge, reject, or escalate (Story 43.3 AC-3).
    /// All decisions are logged with operator, timestamp, and rationale.
    pub async fn adjudicate_dedup(
        &self,
        queue_id: uuid::Uuid,
        decision: &str, // 'approved_merge' | 'rejected_merge' | 'escalated'
        operator: &str,
        rationale: &str,
    ) -> Result<(), Error> {
        sqlx::query(
            r#"
            UPDATE dedup_review_queue
            SET status      = $2,
                reviewed_by = $3,
                reviewed_at = now(),
                rationale   = $4
            WHERE id = $1
              AND status = 'pending_review'
            "#,
        )
        .bind(queue_id)
        .bind(decision)
        .bind(operator)
        .bind(rationale)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Return pending dedup review entries, oldest first.
    pub async fn pending_dedup_reviews(&self) -> Result<Vec<DedupQueueEntry>, Error> {
        let rows = sqlx::query(
            r#"
            SELECT id, canonical_domain, candidate_domain, candidate_name,
                   similarity_score, match_reason, status
            FROM dedup_review_queue
            WHERE status = 'pending_review'
            ORDER BY created_at ASC
            "#,
        )
        .fetch_all(self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| DedupQueueEntry {
                id: r.get("id"),
                canonical_domain: r.get("canonical_domain"),
                candidate_domain: r.get("candidate_domain"),
                candidate_name: r.get("candidate_name"),
                similarity_score: r.get("similarity_score"),
                match_reason: r.get("match_reason"),
                status: r.get("status"),
            })
            .collect())
    }

    // -------------------------------------------------------------------------
    // Leaderboard integration (Story 43.4)
    // -------------------------------------------------------------------------

    /// Resolve display_name and claim_status for a domain, falling back to
    /// the raw domain string and `unclaimed` when no registry entry exists
    /// (Story 43.1 AC-4).
    pub async fn resolve_display(&self, domain: &str) -> Result<(String, String), Error> {
        match self.get(domain).await? {
            Some(e) => Ok((e.display_name, e.claim_status)),
            None => Ok((domain.to_owned(), "unclaimed".to_owned())),
        }
    }
}

// -------------------------------------------------------------------------
// Unicode confusable folding (Story 43.3 AC-1)
// -------------------------------------------------------------------------

/// Fold known unicode confusables to their ASCII equivalents.
///
/// We target the most common script confusables used in IDN homograph attacks
/// — Cyrillic, Greek, and select other scripts. The list is deliberately
/// conservative: only characters that are visually identical (or near-identical)
/// to their Latin counterparts are folded.
///
/// This is not a full Unicode confusable map; it covers the top homograph
/// attack vectors that appear in benchmark entity names.
pub fn fold_confusables(s: &str) -> String {
    s.chars().map(fold_char).collect()
}

fn fold_char(c: char) -> char {
    match c {
        // Cyrillic confusables (most common IDN homograph attack set)
        'А' | 'а' => 'a', // Cyrillic А/а → Latin a
        'В' => 'b',       // Cyrillic В → b
        'С' | 'с' => 'c',
        'Е' | 'е' => 'e',
        'Н' => 'h', // Cyrillic Н → H
        'І' | 'і' => 'i',
        'Ј' | 'ј' => 'j',
        'К' => 'k',
        'М' => 'm',
        'Ν' => 'n',                   // Greek Ν → N
        'Ο' | 'ο' | 'О' | 'о' => 'o', // Greek Ο / Cyrillic О
        'Р' | 'р' => 'p',             // Cyrillic Р → p
        'ρ' => 'p',                   // Greek rho
        'Т' => 't',
        'υ' => 'u',                   // Greek upsilon
        'Х' | 'х' | 'Χ' | 'χ' => 'x', // Cyrillic Х / Greek Χ
        'Υ' | 'Ү' => 'y',
        'Ζ' | 'ζ' => 'z',
        // Full-width ASCII (east-Asian brand names)
        '０'..='９' => char::from_u32('0' as u32 + (c as u32 - '０' as u32)).unwrap_or(c),
        'ａ'..='ｚ' => char::from_u32('a' as u32 + (c as u32 - 'ａ' as u32)).unwrap_or(c),
        'Ａ'..='Ｚ' => char::from_u32('a' as u32 + (c as u32 - 'Ａ' as u32)).unwrap_or(c),
        other => other,
    }
}

// -------------------------------------------------------------------------
// Dedup scoring helper (used by the dedup pass)
// -------------------------------------------------------------------------

/// Compute a similarity score (0–100) between two normalized display names.
///
/// Scoring:
/// * Exact match after normalization → 100 (auto-merge).
/// * Levenshtein distance ≤ 1 → 90 (auto-merge threshold).
/// * After stripping punctuation + collapse whitespace → 85 (review queue).
/// * One is a prefix of the other → 70 (review queue).
/// * Otherwise → 0 (no match).
pub fn display_name_similarity(a: &str, b: &str) -> u8 {
    let na = normalize_display_name(a);
    let nb = normalize_display_name(b);

    if na == nb {
        return 100;
    }
    if levenshtein_le2(&na, &nb) <= 1 {
        return 90;
    }
    let pa = strip_punctuation(&na);
    let pb = strip_punctuation(&nb);
    if pa == pb {
        return 85;
    }
    if pa.starts_with(&pb) || pb.starts_with(&pa) {
        return 70;
    }
    0
}

fn normalize_display_name(s: &str) -> String {
    fold_confusables(&s.to_lowercase())
}

fn strip_punctuation(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Levenshtein distance, capped at 3 for efficiency.
fn levenshtein_le2(a: &str, b: &str) -> usize {
    let av: Vec<char> = a.chars().collect();
    let bv: Vec<char> = b.chars().collect();
    let m = av.len();
    let n = bv.len();
    if m.abs_diff(n) > 2 {
        return 3;
    }
    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr = vec![0usize; n + 1];
    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = usize::from(av[i - 1] != bv[j - 1]);
            curr[j] = (curr[j - 1] + 1).min(prev[j] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
}

// -------------------------------------------------------------------------
// Auto-merge / review-queue threshold constants
// -------------------------------------------------------------------------

/// Similarity score at or above which candidates are auto-merged.
pub const AUTO_MERGE_THRESHOLD: u8 = 90;
/// Similarity score at or above which ambiguous candidates enter the review queue.
pub const REVIEW_QUEUE_THRESHOLD: u8 = 60;

#[cfg(test)]
mod tests {
    use super::*;

    // Story 43.3 AC-1: homoglyph / unicode-confusable detection
    #[test]
    fn cyrillic_a_apple_folds_to_ascii() {
        // "Аpple" where А is Cyrillic (U+0410), rest are Latin
        let homoglyph = "\u{0410}pple";
        assert_eq!(fold_confusables(homoglyph), "apple");
    }

    #[test]
    fn all_ascii_passes_through_unchanged() {
        assert_eq!(fold_confusables("example.com"), "example.com");
    }

    #[test]
    fn normalize_domain_strips_scheme_www_lowercase_and_trailing_slash() {
        assert_eq!(
            EntityRepo::normalize_domain("https://WWW.Example.COM/"),
            "example.com"
        );
    }

    #[test]
    fn normalize_domain_strips_trailing_slash_only() {
        assert_eq!(EntityRepo::normalize_domain("example.com/"), "example.com");
    }

    #[test]
    fn normalize_domain_already_normalized_is_identity() {
        assert_eq!(EntityRepo::normalize_domain("example.com"), "example.com");
    }

    #[test]
    fn normalize_domain_strips_path() {
        assert_eq!(
            EntityRepo::normalize_domain("https://example.com/some/path"),
            "example.com"
        );
    }

    // Story 43.3 AC-1: case-insensitive near-dup normalization
    #[test]
    fn display_name_similarity_exact_match() {
        assert_eq!(display_name_similarity("Acme Inc", "Acme Inc"), 100);
    }

    #[test]
    fn display_name_similarity_case_insensitive() {
        assert_eq!(display_name_similarity("ACME", "acme"), 100);
    }

    #[test]
    fn display_name_similarity_punctuation_stripped() {
        // "Acme Inc" vs "Acme, Inc." — same after strip
        assert_eq!(display_name_similarity("Acme Inc", "Acme, Inc."), 85);
    }

    #[test]
    fn display_name_similarity_edit_distance_1() {
        // "Acmee" vs "Acme" — 1 deletion
        assert_eq!(display_name_similarity("Acmee", "Acme"), 90);
    }

    #[test]
    fn display_name_similarity_unrelated() {
        assert_eq!(display_name_similarity("Acme", "Globex"), 0);
    }

    // Story 43.3 AC-1: Cyrillic А Apple homoglyph → same similarity as ASCII
    #[test]
    fn display_name_similarity_cyrillic_a_matches_ascii_apple() {
        let homoglyph = "\u{0410}pple"; // Cyrillic А + pple
        assert_eq!(display_name_similarity(homoglyph, "Apple"), 100);
    }

    // Threshold boundary: auto-merge at ≥ 90
    #[test]
    fn auto_merge_threshold_boundary() {
        let score = display_name_similarity("Acmee", "Acme");
        assert!(
            score >= AUTO_MERGE_THRESHOLD,
            "expected score {score} >= {AUTO_MERGE_THRESHOLD}"
        );
    }

    // Threshold boundary: review queue for prefix-match (ambiguous but not auto-merge)
    #[test]
    fn review_queue_threshold_boundary() {
        let score = display_name_similarity("Acme", "Acme Inc");
        assert!(
            (REVIEW_QUEUE_THRESHOLD..AUTO_MERGE_THRESHOLD).contains(&score),
            "expected {REVIEW_QUEUE_THRESHOLD} <= score {score} < {AUTO_MERGE_THRESHOLD}"
        );
    }

    // Labeled regression corpus (Story 43.3 AC-6)
    // Each tuple: (a, b, expected_bucket)
    // Buckets: "merge" (>= AUTO), "review" (REVIEW <= x < AUTO), "split" (< REVIEW)
    #[test]
    fn regression_corpus_labeled_pairs() {
        let corpus: &[(&str, &str, &str)] = &[
            ("Apple", "\u{0410}pple", "merge"),     // Cyrillic А → auto-merge
            ("Acme Inc", "Acme, Inc.", "review"),   // punctuation → review
            ("Acme", "Acmee", "merge"),             // edit distance 1 → auto-merge
            ("Acme", "Acme Inc", "review"),         // prefix → review
            ("Globex Corp", "Initech", "split"),    // unrelated
            ("OpenAI", "openai", "merge"),          // case → auto-merge
            ("Google LLC", "Google, LLC", "merge"), // comma = edit-distance 1 → auto-merge
        ];
        for (a, b, expected) in corpus {
            let score = display_name_similarity(a, b);
            let bucket = if score >= AUTO_MERGE_THRESHOLD {
                "merge"
            } else if score >= REVIEW_QUEUE_THRESHOLD {
                "review"
            } else {
                "split"
            };
            assert_eq!(
                bucket, *expected,
                "corpus pair ({a:?}, {b:?}): score={score}, expected bucket={expected}"
            );
        }
    }
}
