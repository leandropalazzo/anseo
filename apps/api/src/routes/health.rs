use axum::routing::get;
use axum::Router;

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/healthz", get(health))
}

/// Phase 2 `/v1/healthz` — same handler, used when /v1/ is auth-gated.
/// Including `healthz` in the auth-gated set lets monitoring infra prove
/// the auth pipeline is up end-to-end, not just the TCP bind.
pub fn v1_router() -> Router<AppState> {
    Router::new().route("/healthz", get(health))
}

async fn health() -> &'static str {
    "ok"
}
