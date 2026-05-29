//! Event-source → webhook-delivery fan-out (Story 12.4).
//!
//! When a `LifecycleEvent` is produced, every webhook whose
//! `event_kinds` JSON array contains the event's kind string gets one
//! `webhook_deliveries` row in `pending`. The dispatcher polls those
//! rows and signs+sends them.
//!
//! Disabled webhooks are skipped here — no row gets created. The
//! operator's `ogeo webhook reenable` is the only path to start
//! receiving new events again.
//!
//! `enqueue_event` is the public entry point producers call. The pure
//! filter logic ([`subscribes`]) is split out so it's unit-testable
//! against the JSONB shape without standing up a DB.

use opengeo_storage::repositories::webhooks::WebhookRow;
use opengeo_storage::Storage;
use serde_json::Value;
use uuid::Uuid;

/// True when this webhook subscribes to `event_kind`. JSONB stored
/// as `["prompt_run.completed", "schedule.missed"]`. Defensive: if
/// the column is somehow a non-array (operator hand-edit), the
/// webhook receives no events rather than panicking.
pub fn subscribes(event_kinds: &Value, event_kind: &str) -> bool {
    match event_kinds {
        Value::Array(arr) => arr.iter().any(|v| v.as_str() == Some(event_kind)),
        _ => false,
    }
}

/// True when this kind is a wire-stable webhook event kind per
/// architecture §5.3. The SSE stream surfaces a broader taxonomy
/// (`schedule.tick_*`); only this curated subset crosses the webhook
/// boundary. Used by producers to short-circuit the fanout DB hit on
/// kinds that no webhook can ever subscribe to.
pub fn is_webhook_eligible(event_kind: &str) -> bool {
    matches!(
        event_kind,
        "prompt_run.completed"
            | "visibility.regression"
            | "schedule.missed"
            | "visibility.anomaly"
            | "citation.anomaly"
    )
}

/// Insert one `pending` delivery row per matching active webhook for the
/// event's project. Returns the freshly-issued delivery row IDs (one per
/// emitted row) so callers can log the fanout count.
///
/// `project_id` is a raw `Uuid` because LifecycleEvent and the
/// inter-process wire layer carry UUIDs, not typed `ProjectId`s. The
/// conversion to `ProjectId` happens at this boundary (the wire→typed
/// projection).
///
/// The payload is serialized once (or accepted pre-serialized) and the
/// same bytes are stored on every delivery row, locking immutability
/// across retries. The dispatcher uses the row's `payload_jsonb` column
/// at delivery time, never re-deriving from primary data.
pub async fn enqueue_event(
    storage: &Storage,
    project_id: Uuid,
    event_kind: &str,
    event_id: Uuid,
    payload: &Value,
) -> Result<Vec<Uuid>, opengeo_storage::Error> {
    let project_id_typed =
        opengeo_core::ProjectId::from_ulid(ulid::Ulid::from_bytes(project_id.into_bytes()));
    let webhooks = storage
        .webhooks()
        .list_for_project(project_id_typed)
        .await?;
    let matching: Vec<WebhookRow> = webhooks
        .into_iter()
        .filter(|w| !w.disabled && subscribes(&w.event_kinds, event_kind))
        .collect();

    if matching.is_empty() {
        return Ok(Vec::new());
    }

    let mut issued = Vec::with_capacity(matching.len());
    for webhook in matching {
        let delivery_id = storage
            .webhook_deliveries()
            .insert_pending(
                webhook.id,
                event_id,
                event_kind,
                1,    // attempt 1 — first emission
                None, // next_attempt_at: NULL → eligible immediately
                payload,
            )
            .await?;
        issued.push(delivery_id);
    }
    Ok(issued)
}

/// Convenience for the producer path: enqueue a `LifecycleEvent` by
/// reading its `kind()`, `project_id()`, `event_id()`, and serializing
/// the whole event as the payload bytes.
///
/// Short-circuits at [`is_webhook_eligible`] so the SSE-only kinds
/// (`schedule.tick_planned` etc.) never hit the DB — webhooks can only
/// subscribe to the 5 architecture §5.3 kinds, so any other kind would
/// always fan-out to zero rows.
pub async fn enqueue_lifecycle_event(
    storage: &Storage,
    event: &crate::events::LifecycleEvent,
) -> Result<Vec<Uuid>, opengeo_storage::Error> {
    let kind = event.kind();
    if !is_webhook_eligible(kind) {
        return Ok(Vec::new());
    }
    let payload = serde_json::to_value(event)
        .map_err(|e| opengeo_storage::Error::Sqlx(sqlx::Error::Decode(Box::new(e))))?;
    enqueue_event(storage, event.project_id(), kind, event.event_id(), &payload).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn subscribes_matches_exact_string() {
        let kinds = json!(["prompt_run.completed", "schedule.missed"]);
        assert!(subscribes(&kinds, "prompt_run.completed"));
        assert!(subscribes(&kinds, "schedule.missed"));
        assert!(!subscribes(&kinds, "visibility.anomaly"));
    }

    #[test]
    fn subscribes_returns_false_for_empty_array() {
        let kinds = json!([]);
        assert!(!subscribes(&kinds, "prompt_run.completed"));
    }

    #[test]
    fn subscribes_returns_false_for_non_array_column() {
        // Defensive: an operator who hand-edited the column to a string
        // or object shouldn't crash the producer; they just get zero
        // events until they fix it.
        assert!(!subscribes(&json!("prompt_run.completed"), "prompt_run.completed"));
        assert!(!subscribes(&json!({"prompt_run.completed": true}), "prompt_run.completed"));
        assert!(!subscribes(&Value::Null, "prompt_run.completed"));
    }

    #[test]
    fn subscribes_is_case_sensitive() {
        let kinds = json!(["prompt_run.completed"]);
        assert!(!subscribes(&kinds, "PROMPT_RUN.COMPLETED"));
        assert!(!subscribes(&kinds, "Prompt_Run.Completed"));
    }

    #[test]
    fn subscribes_treats_non_string_elements_as_no_match() {
        // Defensive: a numeric or boolean element shouldn't match against
        // a string event_kind.
        let kinds = json!(["prompt_run.completed", 42, true, null]);
        assert!(subscribes(&kinds, "prompt_run.completed"));
        assert!(!subscribes(&kinds, "42"));
        assert!(!subscribes(&kinds, "true"));
    }

    #[test]
    fn is_webhook_eligible_matches_arch_5_3() {
        for kind in [
            "prompt_run.completed",
            "visibility.regression",
            "schedule.missed",
            "visibility.anomaly",
            "citation.anomaly",
        ] {
            assert!(is_webhook_eligible(kind), "{kind} should be eligible");
        }
    }

    #[test]
    fn is_webhook_eligible_rejects_sse_only_kinds() {
        // The SSE stream surfaces `schedule.tick_*` and `schedule.debounced`
        // — none of which are webhook-eligible (operators wire them via
        // the live event-stream UI, not webhooks).
        for kind in [
            "schedule.tick_planned",
            "schedule.tick_claimed",
            "schedule.tick_completed",
            "schedule.tick_failed",
            "schedule.tick_capped",
            "schedule.tick_rolled_forward",
            "schedule.debounced",
            "unknown.kind",
        ] {
            assert!(!is_webhook_eligible(kind), "{kind} should NOT be eligible");
        }
    }
}
