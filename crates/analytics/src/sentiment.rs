//! Sentiment aggregation for roadmap Epic 30.

use chrono::NaiveDate;
use opengeo_core::ProjectId;
use opengeo_storage::Storage;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SentimentPoint {
    pub prompt: String,
    pub provider: String,
    pub entity: String,
    pub day: String,
    pub positive: i64,
    pub neutral: i64,
    pub negative: i64,
    pub total: i64,
    pub positive_share: f64,
    pub neutral_share: f64,
    pub negative_share: f64,
    pub average_score: f64,
}

#[derive(Debug, sqlx::FromRow)]
struct SentimentPointRow {
    prompt: String,
    provider: String,
    entity: String,
    day: NaiveDate,
    positive: i64,
    neutral: i64,
    negative: i64,
    total: i64,
    average_score: f64,
}

/// Aggregate mention sentiment per prompt, provider, entity, and day.
///
/// Competitor entities use the same axis as the primary brand because the
/// underlying `mentions.entity` column is canonical for both.
pub async fn sentiment_points(
    storage: &Storage,
    project_id: ProjectId,
    days: i32,
) -> Result<Vec<SentimentPoint>, opengeo_storage::Error> {
    let days = days.clamp(1, 365);
    let rows = sqlx::query_as::<_, SentimentPointRow>(
        r#"
        SELECT
            p.name AS prompt,
            pr.provider AS provider,
            m.entity AS entity,
            pr.started_at::date AS day,
            COUNT(*) FILTER (WHERE m.sentiment_label = 'positive')::bigint AS positive,
            COUNT(*) FILTER (WHERE m.sentiment_label = 'neutral')::bigint AS neutral,
            COUNT(*) FILTER (WHERE m.sentiment_label = 'negative')::bigint AS negative,
            COUNT(*)::bigint AS total,
            COALESCE(AVG(m.sentiment_score), 50)::float8 AS average_score
        FROM mentions m
        JOIN prompt_runs pr ON pr.id = m.prompt_run_id
        JOIN prompts p ON p.id = pr.prompt_id
        WHERE p.project_id = $1
          AND pr.started_at >= now() - make_interval(days => $2)
          AND m.sentiment_label IS NOT NULL
        GROUP BY p.name, pr.provider, m.entity, pr.started_at::date
        ORDER BY day, prompt, provider, entity
        "#,
    )
    .bind(project_id)
    .bind(days)
    .fetch_all(storage.pool())
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let total = row.total.max(1) as f64;
            SentimentPoint {
                prompt: row.prompt,
                provider: row.provider,
                entity: row.entity,
                day: row.day.to_string(),
                positive: row.positive,
                neutral: row.neutral,
                negative: row.negative,
                total: row.total,
                positive_share: row.positive as f64 / total,
                neutral_share: row.neutral as f64 / total,
                negative_share: row.negative as f64 / total,
                average_score: row.average_score,
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_denominator_never_divides_by_zero_shape() {
        let row = SentimentPointRow {
            prompt: "p".into(),
            provider: "openai".into(),
            entity: "Acme".into(),
            day: NaiveDate::from_ymd_opt(2026, 6, 2).unwrap(),
            positive: 0,
            neutral: 0,
            negative: 0,
            total: 0,
            average_score: 50.0,
        };
        let total = row.total.max(1) as f64;
        assert_eq!(row.positive as f64 / total, 0.0);
    }
}
