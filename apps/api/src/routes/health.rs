use axum::routing::get;
use axum::Router;

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/healthz", get(health))
}

async fn health() -> &'static str {
    "ok"
}
