//! Glue between the stateless extractors and the storage layer.
//!
//! The run-execution paths (the API ad-hoc run and the scheduler's tick
//! dispatch) call [`extract_and_persist`] right after persisting a
//! `prompt_runs` row. Without this step the `mentions` / `citations` tables
//! stay empty and every downstream analytic (brand rank, visibility by
//! provider, share of voice) reads as "no data".

use chrono::{DateTime, Utc};
use opengeo_core::ids::{CitationId, ClaimId, MentionId, PromptRunId};
use opengeo_core::Config;
use opengeo_storage::models::{CitationRow, ExtractedClaimRow, MentionRow};
use opengeo_storage::Storage;

use crate::{
    extract_citations, extract_citations_from_annotations, extract_claims, extract_mentions,
    merge_citations,
};

/// Extract mentions, citations, and brand factual claims from a successful run
/// and persist them, linked to `prompt_run_id`. Returns
/// `(mentions, citations, claims)` inserted. A run with no message text (a
/// failure) yields `(0, 0, 0)`.
///
/// Citations come from two surfaces, merged by `(url, domain)`: URLs inlined in
/// the message text, and structured `url_citation` annotations on the raw
/// response (how Perplexity / OpenRouter expose sources). `raw_response` is the
/// provider's JSON body; pass `serde_json::Value::Null` when unavailable.
pub async fn extract_and_persist(
    storage: &Storage,
    config: &Config,
    prompt_run_id: PromptRunId,
    message_text: &str,
    raw_response: &serde_json::Value,
    now: DateTime<Utc>,
) -> Result<(usize, usize, usize), opengeo_storage::Error> {
    let mentions = extract_mentions(message_text, config);
    let citations = merge_citations(
        extract_citations(message_text),
        extract_citations_from_annotations(raw_response),
    );
    let claims = extract_claims(message_text, config);

    let mention_repo = storage.mentions();
    for m in &mentions {
        mention_repo
            .insert(&MentionRow {
                id: MentionId::new(),
                prompt_run_id,
                entity: m.entity.clone(),
                char_offset: m.char_offset as i32,
                rank: m.rank as i32,
                matched_text: m.matched_text.clone(),
                sentiment_label: Some(m.sentiment.label.as_str().to_string()),
                sentiment_score: Some(i16::from(m.sentiment.score)),
                sentiment_lane: Some(m.sentiment.lane.clone()),
                organization_id: None,
                tenant_id: None,
                created_at: now,
            })
            .await?;
    }

    let citation_repo = storage.citations();
    for c in &citations {
        citation_repo
            .insert(&CitationRow {
                id: CitationId::new(),
                prompt_run_id,
                url: c.url.clone(),
                domain: c.domain.clone(),
                frequency: c.frequency as i32,
                source_type: c.source_type.map(|s| s.as_wire_str().to_string()),
                organization_id: None,
                tenant_id: None,
                created_at: now,
            })
            .await?;
    }

    let brand_accuracy_repo = storage.brand_accuracy();
    for claim in &claims {
        brand_accuracy_repo
            .insert_claim(&ExtractedClaimRow {
                id: ClaimId::new(),
                prompt_run_id,
                entity: claim.entity.clone(),
                claim_text: claim.claim_text.clone(),
                claim_kind: claim.claim_kind.clone(),
                char_offset: claim.char_offset.map(|offset| offset as i32),
                confidence: i16::from(claim.confidence),
                extractor_lane: claim.extractor_lane.clone(),
                organization_id: None,
                tenant_id: None,
                created_at: now,
            })
            .await?;
    }

    Ok((mentions.len(), citations.len(), claims.len()))
}
