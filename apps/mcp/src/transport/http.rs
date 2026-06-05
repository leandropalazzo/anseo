//! HTTP+SSE JSON-RPC transport (Story 16.6).
//!
//! Two endpoints:
//!   POST /mcp             — request/response JSON-RPC
//!   GET  /mcp/sse         — SSE push channel (server-sent events)
//!
//! In-flight cap: semaphore of 32 concurrent POST /mcp requests per
//! AD-Phase3-MCP-TransportDefault.
//!
//! Auth: when `require_api_key` is true the `X-OpenGEO-API-Key` header must
//! match `api_key`; mismatch → 401 `{"error":"api_key_required"}`.  This
//! enforces GA criterion mcp-9.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use tokio::sync::Semaphore;
use tokio_stream::wrappers::ReceiverStream;

use crate::dispatch::{Dispatcher, Outbound, ProjectSelector};
use crate::protocol::{ErrorResponse, Id, Request as RpcRequest, PARSE_ERROR};

/// Maximum concurrent in-flight POST /mcp requests.
const MAX_IN_FLIGHT: usize = 32;

/// Shared state injected into every axum handler.
#[derive(Clone)]
struct AppState {
    dispatcher: Arc<Dispatcher>,
    semaphore: Arc<Semaphore>,
    require_api_key: bool,
    api_key: String,
}

/// Start the HTTP+SSE server on `bind`.  Returns when the listener is closed.
pub async fn run(
    dispatcher: Arc<Dispatcher>,
    bind: SocketAddr,
    require_api_key: bool,
    api_key: String,
) -> anyhow::Result<()> {
    let state = AppState {
        dispatcher,
        semaphore: Arc::new(Semaphore::new(MAX_IN_FLIGHT)),
        require_api_key,
        api_key,
    };

    let app = Router::new()
        .route("/mcp", post(handle_post))
        .route("/mcp/sse", get(handle_sse))
        .with_state(state);

    tracing::info!(
        event = "transport.http.listen",
        %bind,
        "HTTP+SSE transport listening"
    );

    let listener = tokio::net::TcpListener::bind(bind).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// POST /mcp
// ---------------------------------------------------------------------------

async fn handle_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    tracing::info!(
        event = "http.request",
        method = "POST",
        path = "/mcp",
        "incoming request"
    );

    // Auth check (mcp-9).
    if state.require_api_key {
        let provided = headers
            .get("X-Anseo-API-Key")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if provided != state.api_key {
            tracing::warn!(
                event = "auth.refused",
                path = "/mcp",
                "missing or invalid X-OpenGEO-API-Key"
            );
            return (
                StatusCode::UNAUTHORIZED,
                axum::Json(serde_json::json!({"error": "api_key_required"})),
            )
                .into_response();
        }
    }

    // Acquire semaphore slot (back-pressure).
    let _permit = match state.semaphore.try_acquire() {
        Ok(p) => p,
        Err(_) => {
            tracing::warn!(event = "semaphore.full", "in-flight cap reached");
            return (
                StatusCode::TOO_MANY_REQUESTS,
                axum::Json(serde_json::json!({"error": "too_many_requests"})),
            )
                .into_response();
        }
    };

    // Story 36.5 AC-2: extract X-Anseo-Project transport hint.
    let transport_hint = headers
        .get("X-Anseo-Project")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_owned());
    let selector = ProjectSelector { transport_hint };

    // Parse JSON-RPC request.
    let reply = match serde_json::from_slice::<RpcRequest>(&body) {
        Ok(req) => state.dispatcher.dispatch_with_selector(req, selector),
        Err(err) => Outbound::Failure(ErrorResponse::new(
            Id::Null,
            PARSE_ERROR,
            format!("parse error: {err}"),
        )),
    };

    match reply {
        Outbound::Success(resp) => {
            let json = serde_json::to_vec(&resp).unwrap_or_default();
            json_response(StatusCode::OK, json)
        }
        Outbound::Failure(err) => {
            let json = serde_json::to_vec(&err).unwrap_or_default();
            // Use 200 for JSON-RPC protocol errors per spec; only use 4xx for
            // transport-level issues (auth, content-type, etc.).
            json_response(StatusCode::OK, json)
        }
        Outbound::Silent => {
            (StatusCode::NO_CONTENT, axum::Json(serde_json::Value::Null)).into_response()
        }
    }
}

fn json_response(status: StatusCode, body: Vec<u8>) -> Response {
    Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .body(Body::from(body))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

// ---------------------------------------------------------------------------
// GET /mcp/sse
// ---------------------------------------------------------------------------

/// SSE endpoint.  Clients that prefer a push channel connect here; the server
/// sends a `connected` event immediately and then streams any future events.
/// In the current shape (no server-push tools) this mainly serves as a
/// presence/keepalive channel.
async fn handle_sse(State(_state): State<AppState>, _req: Request) -> impl IntoResponse {
    tracing::info!(
        event = "http.request",
        method = "GET",
        path = "/mcp/sse",
        "incoming SSE connection"
    );

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<String, Infallible>>(16);

    // Send an initial `connected` event so the client knows the stream is live.
    let _ = tx
        .send(Ok("data: {\"event\":\"connected\"}\n\n".to_string()))
        .await;

    // Drop the sender — the stream will complete after delivering the initial
    // event.  Future stories can retain `tx` for server-push notifications.
    drop(tx);

    let stream = ReceiverStream::new(rx);

    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/event-stream")
        .header("Cache-Control", "no-cache")
        .header("X-Accel-Buffering", "no")
        .body(Body::from_stream(stream))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}
