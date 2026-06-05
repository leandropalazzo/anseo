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

/// Project selector extracted from the transport layer (Story 36.5).
///
/// Precedence (highest first):
///   1. `tools/call` params `project` field (all transports)
///   2. `X-OpenGEO-Project` HTTP header (HTTP/SSE transport)
///   3. The `ApiClient`'s boot-time `OPENGEO_PROJECT_ID` (env fallback)
///
/// Only the first non-empty selector is used; the others are ignored.
#[derive(Debug, Clone, Default)]
pub struct ProjectSelector {
    /// Project name/id from the transport layer (header or env); overridden by
    /// any `project` field in the `tools/call` params.
    pub transport_hint: Option<String>,
}

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

    /// Dispatch a request with no transport-level project hint (stdio path).
    pub fn dispatch(&self, req: Request) -> Outbound {
        self.dispatch_with_selector(req, ProjectSelector::default())
    }

    /// Dispatch a request with an optional transport-level project hint.
    ///
    /// Story 36.5 AC-2: both transports thread the selector through this
    /// method. The `project` field inside `tools/call` params takes precedence
    /// over the transport hint; the transport hint overrides the env default.
    pub fn dispatch_with_selector(&self, req: Request, selector: ProjectSelector) -> Outbound {
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
            "tools/call" => self.handle_tools_call(id.clone(), req.params, &trace_id, &selector),
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
        selector: &ProjectSelector,
    ) -> Result<serde_json::Value, ErrorResponse> {
        let params = params.ok_or_else(|| {
            ErrorResponse::new(id.clone(), INVALID_PARAMS, "missing params")
                .with_data(json!({ "trace_id": trace_id }))
        })?;
        let name = params.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
            ErrorResponse::new(id.clone(), INVALID_PARAMS, "params.name missing")
                .with_data(json!({ "trace_id": trace_id }))
        })?;

        // Story 36.5: `project` in `tools/call` params overrides the transport
        // hint, which in turn overrides the boot-time env default.
        let call_project = params.get("project").and_then(|v| v.as_str());
        let effective_project: Option<&str> =
            call_project.or_else(|| selector.transport_hint.as_deref());

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

        // Build a per-call ApiClient that stamps the resolved project on every
        // loopback request. When no override is present the boot-time project
        // is preserved (no allocation beyond a clone of the inner client).
        let call_api;
        let api: &ApiClient = if let Some(project) = effective_project {
            call_api = self.api.with_project(project);
            &call_api
        } else {
            &self.api
        };

        match tool.call(args, api) {
            Ok(value) => Ok(value),
            Err(McpToolError::NotImplemented) => {
                Err(
                    ErrorResponse::new(id, METHOD_NOT_FOUND, "tool not implemented")
                        .with_data(json!({ "trace_id": trace_id, "tool": name })),
                )
            }
            // AC 36.5-3: unknown project → structured tool error.
            Err(McpToolError::UnknownProject(project)) => {
                Err(
                    ErrorResponse::new(id, INVALID_PARAMS, format!("unknown project: {project}"))
                        .with_data(json!({
                            "trace_id": trace_id,
                            "tool": name,
                            "project": project,
                        })),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::McpToolError;
    use crate::http_client::ApiClient;
    use crate::protocol::Id;

    fn dummy_api() -> ApiClient {
        ApiClient::new(
            "http://127.0.0.1:9999".to_string(),
            String::new(),
            "boot-project".to_string(),
        )
        .expect("dummy ApiClient")
    }

    fn make_tools_call(tool: &str, project: Option<&str>) -> Request {
        let mut params = serde_json::json!({ "name": tool, "arguments": {} });
        if let Some(p) = project {
            params["project"] = serde_json::Value::String(p.to_string());
        }
        Request {
            _jsonrpc: "2.0".to_string(),
            method: "tools/call".to_string(),
            params: Some(params),
            id: Some(Id::Num(1)),
        }
    }

    /// Story 36.5 AC-1: a `tools/call` with a project field in params resolves
    /// to that project (the per-call override path). We can't observe the
    /// `ApiClient` project directly from an error response, but we CAN verify
    /// that a `tools/call` with an unknown tool name yields the expected error
    /// shape with and without a project selector, verifying the dispatcher
    /// routes correctly.
    #[test]
    fn tools_call_unknown_tool_returns_method_not_found() {
        let api = dummy_api();
        let dispatcher = Dispatcher::new(api);
        let req = make_tools_call("no_such_tool", Some("project-alpha"));

        match dispatcher.dispatch(req) {
            Outbound::Failure(err) => {
                assert_eq!(err.error.code, METHOD_NOT_FOUND);
                assert!(err.error.message.contains("unknown tool"));
            }
            other => panic!(
                "expected Outbound::Failure, got {:?}",
                std::mem::discriminant(&other)
            ),
        }
    }

    /// Story 36.5 AC-3: transport hint is honoured when no per-call `project`
    /// is provided. Verified indirectly — the dispatcher must not panic and the
    /// result shape must be well-formed.
    #[test]
    fn transport_hint_threaded_to_dispatch_with_selector() {
        let api = dummy_api();
        let dispatcher = Dispatcher::new(api);
        let selector = ProjectSelector {
            transport_hint: Some("header-project".to_string()),
        };
        let req = make_tools_call("no_such_tool", None);

        match dispatcher.dispatch_with_selector(req, selector) {
            Outbound::Failure(err) => {
                assert_eq!(err.error.code, METHOD_NOT_FOUND);
            }
            other => panic!(
                "expected Outbound::Failure, got {:?}",
                std::mem::discriminant(&other)
            ),
        }
    }

    /// Story 36.5 AC-3: per-call `project` param overrides transport hint
    /// (no observable difference in the error shape, but the resolver must
    /// not panic or fall through).
    #[test]
    fn per_call_project_overrides_transport_hint() {
        let api = dummy_api();
        let dispatcher = Dispatcher::new(api);
        let selector = ProjectSelector {
            transport_hint: Some("transport-project".to_string()),
        };
        // `project` in params takes precedence — dispatcher must not crash.
        let req = make_tools_call("no_such_tool", Some("call-level-project"));

        match dispatcher.dispatch_with_selector(req, selector) {
            Outbound::Failure(err) => {
                assert_eq!(err.error.code, METHOD_NOT_FOUND);
            }
            other => panic!(
                "expected Outbound::Failure, got {:?}",
                std::mem::discriminant(&other)
            ),
        }
    }

    /// Story 36.5 AC-3: `UnknownProject` is mapped to `INVALID_PARAMS` with a
    /// human-readable message and the project name in the error data.
    /// We test this via a mock `Tool` that always returns `UnknownProject`.
    #[test]
    fn unknown_project_error_yields_invalid_params_response() {
        // Build a dispatcher with a single stub tool that always returns
        // UnknownProject, simulating what a real tool would do when the
        // upstream API returns 404 for an unknown project selector.
        struct UnknownProjectTool;
        impl crate::tools::Tool for UnknownProjectTool {
            fn name(&self) -> &'static str {
                "test_unknown_project"
            }
            fn description(&self) -> &'static str {
                "test"
            }
            fn input_schema(&self) -> serde_json::Value {
                serde_json::json!({ "type": "object", "properties": {} })
            }
            fn call(
                &self,
                _args: serde_json::Value,
                api: &ApiClient,
            ) -> Result<serde_json::Value, McpToolError> {
                Err(McpToolError::UnknownProject(
                    api.current_project().to_owned(),
                ))
            }
        }

        let api = ApiClient::new(
            "http://127.0.0.1:9999".to_string(),
            String::new(),
            "boot-project".to_string(),
        )
        .expect("dummy");

        // Inject our stub tool directly without using the registry.
        let dispatcher = Dispatcher {
            tools: vec![Box::new(UnknownProjectTool)],
            api,
            server_version: "0.0.0-test",
        };

        let selector = ProjectSelector {
            transport_hint: Some("bogus-project".to_string()),
        };
        let req = make_tools_call("test_unknown_project", None);

        match dispatcher.dispatch_with_selector(req, selector) {
            Outbound::Failure(err) => {
                assert_eq!(
                    err.error.code, INVALID_PARAMS,
                    "must use INVALID_PARAMS code"
                );
                assert!(
                    err.error.message.contains("unknown project"),
                    "message must name the error class"
                );
                let data = err.error.data.expect("data must be present");
                assert_eq!(
                    data["project"], "bogus-project",
                    "data must carry the project name"
                );
            }
            other => panic!(
                "expected Outbound::Failure, got {:?}",
                std::mem::discriminant(&other)
            ),
        }
    }
}
