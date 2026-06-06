//! Story 41.3 — `list_plugins` + `install_plugin` MCP tools (CLI ⇄ Web ⇄ MCP
//! parity). Both wire to the live plugin surface added in this story:
//!
//!   * `list_plugins`   → `GET  /v1/plugins`         (currently-installed set)
//!   * `install_plugin` → `POST /v1/plugins/install` (verify + record install)
//!
//! These are FIRST-PARTY tools compiled into the MCP binary, not plugin-
//! registered tools — the registry stays the closed set
//! (AD-Phase3-PluginsCannotRegisterMcpTools). They simply add operator-facing
//! parity for the install surface the dashboard and CLI already expose.

use super::Tool;
use crate::error::McpToolError;
use crate::http_client::ApiClient;
use serde_json::json;
use ulid::Ulid;

fn make_upstream_err(msg: &str) -> McpToolError {
    McpToolError::Upstream(anseo_wire_schema::mcp::McpError {
        kind: anseo_wire_schema::mcp::McpErrorKind::InternalError,
        message: msg.to_string(),
        details: None,
        request_id: Ulid::new().to_string(),
        upstream: None,
    })
}

pub struct ListPlugins;

impl Tool for ListPlugins {
    fn name(&self) -> &'static str {
        "list_plugins"
    }

    fn description(&self) -> &'static str {
        "List currently-installed Anseo plugins with their version and signature/verification status."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })
    }

    #[allow(clippy::result_large_err)]
    fn call(
        &self,
        _args: serde_json::Value,
        api: &ApiClient,
    ) -> Result<serde_json::Value, McpToolError> {
        let resp = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async { api.get("/v1/plugins").send().await })
        })
        .map_err(|e| make_upstream_err(&e.to_string()))?;

        if !resp.status().is_success() {
            return Err(make_upstream_err(&format!(
                "upstream /v1/plugins returned {}",
                resp.status()
            )));
        }

        let body: serde_json::Value = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async { resp.json().await })
        })
        .map_err(|e| make_upstream_err(&format!("failed to parse response: {e}")))?;

        Ok(json!({
            "plugins": body.get("plugins").cloned().unwrap_or_else(|| json!([])),
            "trace_id": Ulid::new().to_string(),
        }))
    }
}

pub struct InstallPlugin;

impl Tool for InstallPlugin {
    fn name(&self) -> &'static str {
        "install_plugin"
    }

    fn description(&self) -> &'static str {
        "Install a plugin from the live registry by id (namespace/name). Verifies the artifact's checksum + signature; set acknowledge_unsigned to allow an unsigned plugin."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "Registry plugin id, e.g. `namespace/name`."
                },
                "acknowledge_unsigned": {
                    "type": "boolean",
                    "description": "Allow installing an unsigned plugin (default false)."
                }
            },
            "required": ["id"],
            "additionalProperties": false
        })
    }

    #[allow(clippy::result_large_err)]
    fn call(
        &self,
        args: serde_json::Value,
        api: &ApiClient,
    ) -> Result<serde_json::Value, McpToolError> {
        let id = args
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| make_upstream_err("invalid params: `id` is required"))?;
        let acknowledge_unsigned = args
            .get("acknowledge_unsigned")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let body = json!({ "id": id, "acknowledge_unsigned": acknowledge_unsigned });

        let resp = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async { api.post("/v1/plugins/install").json(&body).send().await })
        })
        .map_err(|e| make_upstream_err(&e.to_string()))?;

        if !resp.status().is_success() {
            return Err(make_upstream_err(&format!(
                "upstream /v1/plugins/install returned {}",
                resp.status()
            )));
        }

        let result: serde_json::Value = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async { resp.json().await })
        })
        .map_err(|e| make_upstream_err(&format!("failed to parse response: {e}")))?;

        Ok(result)
    }
}
