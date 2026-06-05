//! OSS storage substrate for Epic 34 brand-accuracy monitoring.
//!
//! This crate owns extracted claims and ground-truth facts. Premium
//! hallucination evaluation can read these rows, but storage deliberately has
//! no dependency on the commercial evaluator.

use anseo_core::ids::{ClaimId, GroundTruthFactId, ProjectId, PromptRunId};
use sqlx::PgPool;

use crate::error::Error;
use crate::models::{ExtractedClaimRow, GroundTruthFactRow};

pub struct BrandAccuracyRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> BrandAccuracyRepo<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn insert_claim(&self, row: &ExtractedClaimRow) -> Result<ClaimId, Error> {
        sqlx::query(
            r#"
            INSERT INTO extracted_claims (
                id, prompt_run_id, entity, claim_text, claim_kind, char_offset,
                confidence, extractor_lane, organization_id, tenant_id, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
        )
        .bind(row.id)
        .bind(row.prompt_run_id)
        .bind(&row.entity)
        .bind(&row.claim_text)
        .bind(&row.claim_kind)
        .bind(row.char_offset)
        .bind(row.confidence)
        .bind(&row.extractor_lane)
        .bind(row.organization_id)
        .bind(row.tenant_id)
        .bind(row.created_at)
        .execute(self.pool)
        .await?;
        Ok(row.id)
    }

    pub async fn list_claims_by_run(
        &self,
        prompt_run_id: PromptRunId,
    ) -> Result<Vec<ExtractedClaimRow>, Error> {
        let rid = uuid::Uuid::from_bytes(prompt_run_id.into_ulid().to_bytes());
        let rows = sqlx::query_as::<_, ExtractedClaimRow>(
            r#"
            SELECT id, prompt_run_id, entity, claim_text, claim_kind, char_offset,
                   confidence, extractor_lane, organization_id, tenant_id, created_at
            FROM extracted_claims
            WHERE prompt_run_id = $1
            ORDER BY COALESCE(char_offset, 2147483647), id
            "#,
        )
        .bind(rid)
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn get_claim(&self, id: ClaimId) -> Result<Option<ExtractedClaimRow>, Error> {
        let cid = uuid::Uuid::from_bytes(id.into_ulid().to_bytes());
        let row = sqlx::query_as::<_, ExtractedClaimRow>(
            r#"
            SELECT id, prompt_run_id, entity, claim_text, claim_kind, char_offset,
                   confidence, extractor_lane, organization_id, tenant_id, created_at
            FROM extracted_claims
            WHERE id = $1
            "#,
        )
        .bind(cid)
        .fetch_optional(self.pool)
        .await?;
        Ok(row)
    }

    pub async fn upsert_ground_truth_fact(
        &self,
        row: &GroundTruthFactRow,
    ) -> Result<GroundTruthFactId, Error> {
        sqlx::query(
            r#"
            INSERT INTO brand_ground_truth_facts (
                id, project_id, entity, fact_key, fact_value, source_url,
                source_label, source_type, valid_from, valid_to,
                organization_id, tenant_id, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            ON CONFLICT (project_id, entity, fact_key)
            DO UPDATE SET
                fact_value = EXCLUDED.fact_value,
                source_url = EXCLUDED.source_url,
                source_label = EXCLUDED.source_label,
                source_type = EXCLUDED.source_type,
                valid_from = EXCLUDED.valid_from,
                valid_to = EXCLUDED.valid_to,
                organization_id = EXCLUDED.organization_id,
                tenant_id = EXCLUDED.tenant_id
            "#,
        )
        .bind(row.id)
        .bind(row.project_id)
        .bind(&row.entity)
        .bind(&row.fact_key)
        .bind(&row.fact_value)
        .bind(&row.source_url)
        .bind(&row.source_label)
        .bind(&row.source_type)
        .bind(row.valid_from)
        .bind(row.valid_to)
        .bind(row.organization_id)
        .bind(row.tenant_id)
        .bind(row.created_at)
        .execute(self.pool)
        .await?;
        Ok(row.id)
    }

    /// Project-scoped recent claims for the brand-accuracy dashboard
    /// (Epic 34 Story 3). Joins claims → prompt_runs → prompts to scope by
    /// project and window by claim `created_at`. Newest first.
    pub async fn list_recent_claims_for_project(
        &self,
        project_id: ProjectId,
        days: i64,
        limit: i64,
    ) -> Result<Vec<ExtractedClaimRow>, Error> {
        let pid = uuid::Uuid::from_bytes(project_id.into_ulid().to_bytes());
        let rows = sqlx::query_as::<_, ExtractedClaimRow>(
            r#"
            SELECT c.id, c.prompt_run_id, c.entity, c.claim_text, c.claim_kind,
                   c.char_offset, c.confidence, c.extractor_lane,
                   c.organization_id, c.tenant_id, c.created_at
            FROM extracted_claims c
            JOIN prompt_runs pr ON c.prompt_run_id = pr.id
            JOIN prompts p ON pr.prompt_id = p.id
            WHERE p.project_id = $1
              AND c.created_at >= now() - make_interval(days => $2::int)
            ORDER BY c.created_at DESC, c.id DESC
            LIMIT $3
            "#,
        )
        .bind(pid)
        .bind(days as i32)
        .bind(limit)
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }

    /// Count of ground-truth facts configured for a project — the denominator
    /// for hallucination judgment.
    pub async fn count_ground_truth_for_project(
        &self,
        project_id: ProjectId,
    ) -> Result<i64, Error> {
        let pid = uuid::Uuid::from_bytes(project_id.into_ulid().to_bytes());
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM brand_ground_truth_facts WHERE project_id = $1")
                .bind(pid)
                .fetch_one(self.pool)
                .await?;
        Ok(count.0)
    }

    pub async fn list_ground_truth_for_entity(
        &self,
        project_id: ProjectId,
        entity: &str,
    ) -> Result<Vec<GroundTruthFactRow>, Error> {
        let pid = uuid::Uuid::from_bytes(project_id.into_ulid().to_bytes());
        let rows = sqlx::query_as::<_, GroundTruthFactRow>(
            r#"
            SELECT id, project_id, entity, fact_key, fact_value, source_url,
                   source_label, source_type, valid_from, valid_to,
                   organization_id, tenant_id, created_at
            FROM brand_ground_truth_facts
            WHERE project_id = $1 AND entity = $2
            ORDER BY fact_key, id
            "#,
        )
        .bind(pid)
        .bind(entity)
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }
}
