//! JSON-RPC method router.
//!
//! Owns the tool registry and translates inbound method calls into either a
//! [`Response`] or [`ErrorResponse`]. Tool handlers are stubs in Story 16.1
//! (return `-32601 tool not implemented`).

use serde_json::json;
use ulid::Ulid;

use crate::error::McpToolError;
use crate::http_client::ApiClient;
use crate::protocol::{ErrorResponse, Id, Request, Response, INVALID_PARAMS, METHOD_NOT_FOUND};
use crate::tools::{self, Tool};

/// Output of dispatching one request — either a success or error response, or
/// `None` for notifications (which never produce a reply on the wire).
pub enum Outbound {
    Success(Response),
    Failure(ErrorResponse),
    Silent,
}

pub struct Dispatcher {
    tools: Vec<Box<dyn Tool>>,
    api: ApiClient,
    server_version: &'static str,
}

impl Dispatcher {
    pub fn new(api: ApiClient) -> Self {
        Self {
            tools: tools::registry(),
            api,
            server_version: env!("CARGO_PKG_VERSION"),
        }
    }

    /// Generate a fresh ULID trace id. Surfaced on every response so MCP
    /// clients can correlate with `/v1` server logs.
    fn new_trace_id() -> String {
        Ulid::new().to_string()
    }

    pub fn dispatch(&self, req: Request) -> Outbound {
        // Notifications (no `id`) never reply. Per JSON-RPC 2.0 §4.1.
        let is_notification = req.id.is_none();
        let id = req.id.unwrap_or(Id::Null);

        let trace_id = Self::new_trace_id();

        let result: Result<serde_json::Value, ErrorResponse> = match req.method.as_str() {
            "initialize" => Ok(self.handle_initialize(&trace_id)),
            "initialized" | "notifications/initialized" => {
                // Client-sent notification; no body expected.
                if is_notification {
                    return Outbound::Silent;
                }
                Ok(json!({}))
            }
            "tools/list" => Ok(self.handle_tools_list(&trace_id)),
            "tools/call" => self.handle_tools_call(id.clone(), req.params, &trace_id),
            "ping" => Ok(json!({})),
            other => Err(ErrorResponse::new(
                id.clone(),
                METHOD_NOT_FOUND,
                format!("method not found: {other}"),
            )
            .with_data(json!({ "trace_id": trace_id }))),
        };

        if is_notification {
            return Outbound::Silent;
        }

        match result {
            Ok(value) => Outbound::Success(Response::ok(id, value)),
            Err(err) => Outbound::Failure(err),
        }
    }

    fn handle_initialize(&self, trace_id: &str) -> serde_json::Value {
        // architecture-phase3-mcp-server.md §2.3
        json!({
            "protocolVersion": "2024-11-05",
            "serverInfo": { "name": "opengeo-mcp", "version": self.server_version },
            "capabilities": {
                "tools":      { "listChanged": false },
                "resources":  null,
                "prompts":    null,
                "logging":    { "setLevel": true }
            },
            "trace_id": trace_id,
        })
    }

    fn handle_tools_list(&self, trace_id: &str) -> serde_json::Value {
        let tools: Vec<serde_json::Value> = self
            .tools
            .iter()
            .map(|t| {
                json!({
                    "name":        t.name(),
                    "description": t.description(),
                    "inputSchema": t.input_schema(),
                })
            })
            .collect();
        json!({
            "tools": tools,
            "trace_id": trace_id,
        })
    }

    fn handle_tools_call(
        &self,
        id: Id,
        params: Option<serde_json::Value>,
        trace_id: &str,
    ) -> Result<serde_json::Value, ErrorResponse> {
        let params = params.ok_or_else(|| {
            ErrorResponse::new(id.clone(), INVALID_PARAMS, "missing params")
                .with_data(json!({ "trace_id": trace_id }))
        })?;
        let name = params.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
            ErrorResponse::new(id.clone(), INVALID_PARAMS, "params.name missing")
                .with_data(json!({ "trace_id": trace_id }))
        })?;
        let args = params
            .get("arguments")
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        let tool = self
            .tools
            .iter()
            .find(|t| t.name() == name)
            .ok_or_else(|| {
                ErrorResponse::new(
                    id.clone(),
                    METHOD_NOT_FOUND,
                    format!("unknown tool: {name}"),
                )
                .with_data(json!({ "trace_id": trace_id }))
            })?;

        match tool.call(args, &self.api) {
            Ok(value) => Ok(value),
            Err(McpToolError::NotImplemented) => {
                Err(
                    ErrorResponse::new(id, METHOD_NOT_FOUND, "tool not implemented")
                        .with_data(json!({ "trace_id": trace_id, "tool": name })),
                )
            }
            Err(McpToolError::Upstream(env)) => {
                let data = serde_json::to_value(&env).unwrap_or(json!({}));
                Err(
                    ErrorResponse::new(id, crate::protocol::INTERNAL_ERROR, env.message.clone())
                        .with_data(json!({ "trace_id": trace_id, "tool": name, "upstream": data })),
                )
            }
        }
    }
}
