use std::sync::Arc;

use anseo_api::{router, AppState};
use anseo_core::ProjectId;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

fn build_router() -> axum::Router {
    let lazy_pool = sqlx::PgPool::connect_lazy("postgres://anseo:anseo@127.0.0.1:1/__suite_test__")
        .expect("connect_lazy never IOs synchronously");
    let storage = Arc::new(anseo_storage::Storage::from_pool(lazy_pool));
    let (events, _rx) = anseo_scheduler::worker::event_channel();
    let state = AppState {
        storage,
        project_id: ProjectId::new(),
        events,
        config: None,
        provider_registry: None,
        configured_project: Arc::new("default".to_string()),
        setup_install_state: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        serve_info: None,
        loaded_plugins: Arc::new(Vec::new()),
    };
    router(state)
}

#[tokio::test]
async fn suite_prompts_endpoint_requires_auth() {
    let app = build_router();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/suite/prompts")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
