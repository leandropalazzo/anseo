//! Anonymous benchmark contribution outbox (Epic 40 / Story 40.4).
//!
//! Ingested runs can opt into the anonymous benchmark path. The handler already
//! redacts and seals a [`SealedContribution`]; this repo persists that envelope
//! durably so later upload / aggregation work can consume it without ever
//! storing cleartext benchmark payloads.

use anseo_core::ids::{ProjectId, PromptRunId};
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::error::Error;

#[derive(Debug, Clone)]
pub struct AnonymousContributionToStore {
    pub prompt_run_id: PromptRunId,
    pub project_id: ProjectId,
    pub project_hmac: String,
    pub consent_record_id: Uuid,
    pub terms_version: String,
    pub sealed_payload: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct AnonymousContributionRow {
    pub id: Uuid,
    pub prompt_run_id: PromptRunId,
    pub project_id: ProjectId,
    pub project_hmac: String,
    pub consent_record_id: Uuid,
    pub terms_version: String,
    pub sealed_payload: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

pub struct AnonymousContributionRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> AnonymousContributionRepo<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn insert(&self, row: &AnonymousContributionToStore) -> Result<Uuid, Error> {
        let mut tx = self.pool.begin().await?;
        let id = self.insert_in_tx(&mut tx, row).await?;
        tx.commit().await?;
        Ok(id)
    }

    pub async fn insert_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        row: &AnonymousContributionToStore,
    ) -> Result<Uuid, Error> {
        let id = Uuid::new_v4();
        let result = sqlx::query(
            r#"
            INSERT INTO anonymous_contributions (
                id, prompt_run_id, project_id, project_hmac, consent_record_id,
                terms_version, sealed_payload
            )
            SELECT
                $1, $2, $3, $4, $5, $6, $7::jsonb
            FROM benchmark_consent bc
            WHERE bc.id = $5
              AND bc.project_id = $3
              AND bc.tier = 'anonymous'
              AND bc.terms_version = $6
            "#,
        )
        .bind(id)
        .bind(row.prompt_run_id)
        .bind(row.project_id)
        .bind(&row.project_hmac)
        .bind(row.consent_record_id)
        .bind(&row.terms_version)
        .bind(&row.sealed_payload)
        .execute(&mut **tx)
        .await?;
        if result.rows_affected() != 1 {
            return Err(Error::NotFound);
        }
        Ok(id)
    }

    pub async fn by_prompt_run(
        &self,
        prompt_run_id: PromptRunId,
    ) -> Result<Option<AnonymousContributionRow>, Error> {
        let row = sqlx::query(
            r#"
            SELECT id, prompt_run_id, project_id, project_hmac, consent_record_id,
                   terms_version, sealed_payload, created_at
            FROM anonymous_contributions
            WHERE prompt_run_id = $1
            "#,
        )
        .bind(prompt_run_id)
        .fetch_optional(self.pool)
        .await?;
        row.map(map_row).transpose()
    }
}

fn map_row(row: sqlx::postgres::PgRow) -> Result<AnonymousContributionRow, Error> {
    Ok(AnonymousContributionRow {
        id: row.try_get("id")?,
        prompt_run_id: row.try_get("prompt_run_id")?,
        project_id: row.try_get("project_id")?,
        project_hmac: row.try_get("project_hmac")?,
        consent_record_id: row.try_get("consent_record_id")?,
        terms_version: row.try_get("terms_version")?,
        sealed_payload: row.try_get("sealed_payload")?,
        created_at: row.try_get("created_at")?,
    })
}
