use sqlx::PgPool;
use uuid::Uuid;

use crate::error::Error;

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct NotificationRow {
    pub id: Uuid,
    pub org_id: Uuid,
    pub operator_id: Option<Uuid>,
    pub kind: String,
    pub subject: String,
    pub body_text: String,
    pub read_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub struct NotificationsRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> NotificationsRepo<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn create(
        &self,
        org_id: Uuid,
        operator_id: Option<Uuid>,
        kind: &str,
        subject: &str,
        body_text: &str,
    ) -> Result<Uuid, Error> {
        let id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO operator_notifications
                (org_id, operator_id, kind, subject, body_text)
            VALUES
                ($1, $2, $3::notification_kind, $4, $5)
            RETURNING id
            "#,
        )
        .bind(org_id)
        .bind(operator_id)
        .bind(kind)
        .bind(subject)
        .bind(body_text)
        .fetch_one(self.pool)
        .await?;
        Ok(id)
    }

    pub async fn list_for_org(
        &self,
        org_id: Uuid,
        limit: i64,
    ) -> Result<Vec<NotificationRow>, Error> {
        let rows: Vec<NotificationRow> = sqlx::query_as(
            r#"
            SELECT
                id, org_id, operator_id,
                kind::TEXT AS kind,
                subject, body_text, read_at, created_at
            FROM operator_notifications
            WHERE org_id = $1
            ORDER BY created_at DESC
            LIMIT $2
            "#,
        )
        .bind(org_id)
        .bind(limit)
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn mark_read(&self, id: Uuid, org_id: Uuid) -> Result<bool, Error> {
        let rows = sqlx::query(
            r#"
            UPDATE operator_notifications
            SET    read_at = now()
            WHERE  id = $1 AND org_id = $2 AND read_at IS NULL
            "#,
        )
        .bind(id)
        .bind(org_id)
        .execute(self.pool)
        .await?
        .rows_affected();
        Ok(rows > 0)
    }
}
