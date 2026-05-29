//! Phase 2 Story 12.1 / 12.3 — OpenAPI generator.
//!
//! Writes a stable `openapi.json` to stdout describing the Phase 2
//! `/v1/*` REST surface. This is the substrate the Story 12.3 SDK
//! codegen pipeline consumes; the byte-equal output of this binary is
//! what `infra/codegen/tests/drift.sh` will compare against committed
//! `crates/wire-schema/openapi.json`.
//!
//! Current scope: hand-rolled minimal spec describing the routes that
//! exist on main today (`/v1/runs`, `/v1/citations/summary`,
//! `/v1/visibility/trend`, `/v1/healthz`, `/v1/prompt-runs`,
//! `/v1/projects/:project_id/events`). A future round wires `utoipa`
//! annotations on every handler so the spec is generated from the
//! source-of-truth Rust types rather than hand-maintained.
//!
//! Usage: `cargo run -p opengeo-wire-schema --bin gen-openapi`

use serde_json::json;

fn main() {
    let spec = build_spec();
    let pretty = serde_json::to_string_pretty(&spec).expect("json serialization");
    println!("{pretty}");
}

fn build_spec() -> serde_json::Value {
    json!({
        "openapi": "3.0.3",
        "info": {
            "title": "OpenGEO Public REST API",
            "version": "0.2.0",
            "description": "Phase 2 v1 surface — read endpoints, prompt run submission, and the SSE event stream. Auth: X-OpenGEO-API-Key header carrying an `ogeo_<32 chars>` key."
        },
        "servers": [
            {
                "url": "https://api.opengeo.dev",
                "description": "Hosted production"
            },
            {
                "url": "http://127.0.0.1:8788",
                "description": "Local Compose stack"
            }
        ],
        "security": [
            { "ApiKeyAuth": [] }
        ],
        "components": {
            "securitySchemes": {
                "ApiKeyAuth": {
                    "type": "apiKey",
                    "in": "header",
                    "name": "X-OpenGEO-API-Key"
                }
            },
            "responses": {
                "Unauthorized": {
                    "description": "Missing, malformed, or revoked X-OpenGEO-API-Key. The body shape is intentionally opaque so an unauthenticated caller cannot distinguish 'wrong key' from 'auth backend down'.",
                    "content": {
                        "application/json": {
                            "schema": { "$ref": "#/components/schemas/Error" }
                        }
                    }
                }
            },
            "schemas": {
                "Error": {
                    "type": "object",
                    "required": ["error", "message"],
                    "properties": {
                        "error": { "type": "string" },
                        "message": { "type": "string" }
                    }
                },
                "RunListResponse": {
                    "type": "object",
                    "properties": {
                        "runs": { "type": "array", "items": { "type": "object" } }
                    }
                },
                "VisibilityTrendResponse": {
                    "type": "object",
                    "properties": {
                        "points": { "type": "array", "items": { "type": "object" } }
                    }
                },
                "CitationSummaryResponse": {
                    "type": "object",
                    "properties": {
                        "domains": { "type": "array", "items": { "type": "object" } }
                    }
                },
                "CreatePromptRunRequest": {
                    "type": "object",
                    "required": ["prompt_name", "provider"],
                    "properties": {
                        "prompt_name": { "type": "string", "description": "Slug-safe prompt identifier declared in opengeo.yaml." },
                        "provider": {
                            "type": "string",
                            "enum": ["openai", "anthropic", "gemini", "perplexity", "grok", "mistral", "openrouter", "mock"]
                        },
                        "triggered_by": { "type": "string", "nullable": true }
                    }
                },
                "CreatePromptRunResponse": {
                    "type": "object",
                    "required": ["status", "run_id", "project_id", "prompt_name", "provider", "dispatched_at"],
                    "properties": {
                        "status": { "type": "string" },
                        "run_id": { "type": "string" },
                        "project_id": { "type": "string" },
                        "prompt_name": { "type": "string" },
                        "provider": { "type": "string" },
                        "dispatched_at": { "type": "string", "format": "date-time" }
                    }
                }
            }
        },
        "paths": {
            "/v1/healthz": {
                "get": {
                    "operationId": "healthz",
                    "summary": "Health probe",
                    "responses": {
                        "200": {
                            "description": "OK",
                            "content": {
                                "text/plain": { "schema": { "type": "string", "example": "ok" } }
                            }
                        },
                        "401": { "$ref": "#/components/responses/Unauthorized" }
                    }
                }
            },
            "/v1/runs": {
                "get": {
                    "operationId": "listRuns",
                    "summary": "List recent Prompt Runs",
                    "parameters": [
                        { "name": "limit", "in": "query", "schema": { "type": "integer", "minimum": 1, "maximum": 500 } },
                        { "name": "offset", "in": "query", "schema": { "type": "integer", "minimum": 0 } }
                    ],
                    "responses": {
                        "200": {
                            "description": "OK",
                            "content": { "application/json": { "schema": { "$ref": "#/components/schemas/RunListResponse" } } }
                        },
                        "401": { "$ref": "#/components/responses/Unauthorized" }
                    }
                }
            },
            "/v1/citations/summary": {
                "get": {
                    "operationId": "citationSummary",
                    "parameters": [
                        { "name": "limit", "in": "query", "schema": { "type": "integer" } }
                    ],
                    "responses": {
                        "200": {
                            "description": "OK",
                            "content": { "application/json": { "schema": { "$ref": "#/components/schemas/CitationSummaryResponse" } } }
                        },
                        "401": { "$ref": "#/components/responses/Unauthorized" }
                    }
                }
            },
            "/v1/visibility/trend": {
                "get": {
                    "operationId": "visibilityTrend",
                    "parameters": [
                        { "name": "prompt", "in": "query", "required": true, "schema": { "type": "string" } },
                        { "name": "days", "in": "query", "schema": { "type": "integer" } }
                    ],
                    "responses": {
                        "200": {
                            "description": "OK",
                            "content": { "application/json": { "schema": { "$ref": "#/components/schemas/VisibilityTrendResponse" } } }
                        },
                        "401": { "$ref": "#/components/responses/Unauthorized" }
                    }
                }
            },
            "/v1/prompt-runs": {
                "post": {
                    "operationId": "createPromptRun",
                    "summary": "Dispatch a one-shot prompt run for an already-declared Prompt and Provider.",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": { "schema": { "$ref": "#/components/schemas/CreatePromptRunRequest" } }
                        }
                    },
                    "responses": {
                        "202": {
                            "description": "Accepted",
                            "content": { "application/json": { "schema": { "$ref": "#/components/schemas/CreatePromptRunResponse" } } }
                        },
                        "400": {
                            "description": "Validation error",
                            "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Error" } } }
                        },
                        "401": { "$ref": "#/components/responses/Unauthorized" }
                    }
                }
            },
            "/v1/projects/{project_id}/events": {
                "get": {
                    "operationId": "subscribeEvents",
                    "summary": "Server-Sent Events stream of ARCH-17 lifecycle events for one project.",
                    "parameters": [
                        { "name": "project_id", "in": "path", "required": true, "schema": { "type": "string", "format": "uuid" } }
                    ],
                    "responses": {
                        "200": {
                            "description": "text/event-stream subscription",
                            "content": { "text/event-stream": {} }
                        },
                        "401": { "$ref": "#/components/responses/Unauthorized" },
                        "403": {
                            "description": "Cross-project subscription attempt; key authorized for a different project_id.",
                            "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Error" } } }
                        }
                    }
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_has_openapi_field_and_components() {
        let spec = build_spec();
        assert_eq!(spec["openapi"], "3.0.3");
        assert!(spec["components"]["schemas"].is_object());
        assert!(spec["components"]["securitySchemes"]["ApiKeyAuth"].is_object());
    }

    #[test]
    fn spec_declares_x_opengeo_api_key_security_scheme() {
        let spec = build_spec();
        assert_eq!(
            spec["components"]["securitySchemes"]["ApiKeyAuth"]["name"],
            "X-OpenGEO-API-Key"
        );
    }

    #[test]
    fn spec_includes_phase_2_paths() {
        let spec = build_spec();
        for path in [
            "/v1/healthz",
            "/v1/runs",
            "/v1/citations/summary",
            "/v1/visibility/trend",
            "/v1/prompt-runs",
            "/v1/projects/{project_id}/events",
        ] {
            assert!(
                spec["paths"][path].is_object(),
                "spec missing path {path}"
            );
        }
    }

    #[test]
    fn prompt_runs_post_returns_202() {
        let spec = build_spec();
        let post = &spec["paths"]["/v1/prompt-runs"]["post"];
        assert!(post["responses"]["202"].is_object());
    }

    #[test]
    fn all_response_refs_resolve_to_declared_components() {
        // Every `$ref: "#/components/responses/X"` in the spec must
        // have a matching component declaration. A drift here would
        // produce a spec that fails any conforming OpenAPI validator.
        let spec = build_spec();
        let serialized = serde_json::to_string(&spec).unwrap();
        let declared: Vec<String> = spec["components"]["responses"]
            .as_object()
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default();
        let needle = "#/components/responses/";
        let mut start = 0;
        while let Some(pos) = serialized[start..].find(needle) {
            let after = start + pos + needle.len();
            let end = serialized[after..]
                .find('"')
                .map(|e| after + e)
                .unwrap_or(serialized.len());
            let referenced = serialized[after..end].to_string();
            assert!(
                declared.contains(&referenced),
                "spec $refs `{referenced}` but no such component declared"
            );
            start = end;
        }
        // Pin the explicit Unauthorized name too so a future rename
        // surfaces in both places.
        assert!(declared.iter().any(|n| n == "Unauthorized"));
    }

    #[test]
    fn spec_serializes_to_pretty_json() {
        // Pin that the spec is serializable as pretty JSON — the CI
        // drift check (Story 12.3 follow-up) compares byte-equal
        // against the committed openapi.json.
        let spec = build_spec();
        let pretty = serde_json::to_string_pretty(&spec).unwrap();
        assert!(pretty.contains("\n"), "pretty JSON must have newlines");
        // Round-trip back to value.
        let back: serde_json::Value = serde_json::from_str(&pretty).unwrap();
        assert_eq!(back, spec);
    }
}
