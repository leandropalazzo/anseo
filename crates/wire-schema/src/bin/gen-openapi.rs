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
            "version": "0.3.1",
            "description": "Phase 2 v1 surface — read endpoints, prompt run submission, and the SSE event stream. Auth: X-OpenGEO-API-Key header carrying an `ogeo_<32 chars>` key. Story 0.11 (Phase 3 substrate) adds the optional X-OpenGEO-Project header — Phase 2 accepts it for forward compatibility; Phase 4 will require it. Story 15.1 adds three `/v1/setup/*` endpoints (deployment UX substrate)."
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
            "parameters": {
                "ProjectHeader": {
                    "name": "X-OpenGEO-Project",
                    "in": "header",
                    "required": false,
                    "schema": { "type": "string" },
                    "description": "Story 0.11 substrate (Phase 3 decision L2). Identifies the target project. Phase 2 single-project deployments accept the header for forward-compatibility: the value must equal the configured project name (case-insensitive after trim) or the reserved sentinel `default`. Mismatching values return 403 `project_not_found`. Absent header is accepted in Phase 2 (with a one-time per-process WARN log) and will become required in Phase 4 multi-project mode."
                }
            },
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
                "ComparisonsResponse": {
                    "type": "object",
                    "description": "Story 0.8 `GET /v1/comparisons` matrix payload — mirrors the MCP CompareBrandsOutput shape (architecture-phase3-mcp-server.md §3.3). Determinism contract: rows ordered (prompt_name ASC, provider ASC); cells ordered [brand, ...competitors_in_caller_order]; absent subjects carry ranking:null (NOT omitted).",
                    "required": ["window", "brand", "competitors", "rows", "trace_id"],
                    "properties": {
                        "window": { "type": "string", "enum": ["7d", "30d", "all"] },
                        "brand": { "type": "string" },
                        "competitors": { "type": "array", "items": { "type": "string" } },
                        "rows": { "type": "array", "items": { "$ref": "#/components/schemas/ComparisonRow" } },
                        "trace_id": { "type": "string" }
                    }
                },
                "ComparisonRow": {
                    "type": "object",
                    "required": ["prompt_id", "prompt_name", "provider", "cells"],
                    "properties": {
                        "prompt_id": { "type": "string", "description": "ULID." },
                        "prompt_name": { "type": "string" },
                        "provider": { "type": "string" },
                        "cells": { "type": "array", "items": { "$ref": "#/components/schemas/ComparisonCell" } }
                    }
                },
                "ComparisonCell": {
                    "type": "object",
                    "required": ["subject", "mention_count"],
                    "properties": {
                        "subject": { "type": "string" },
                        "ranking": { "type": "integer", "nullable": true, "minimum": 1 },
                        "mention_count": { "type": "integer", "minimum": 0 }
                    }
                },
                "SetupStatus": {
                    "type": "object",
                    "description": "Story 15.1 — best-effort status probe across all deployment surfaces. Always returned 200; individual sections carry `state: \"unknown\"` + an `error` string on failure (per-probe timeout: 1s; 500ms for Docker).",
                    "required": ["postgres", "clickhouse", "worker", "webhook_target", "api_keys", "docker"],
                    "properties": {
                        "postgres": {
                            "type": "object",
                            "required": ["state"],
                            "properties": {
                                "state": { "type": "string", "enum": ["healthy", "degraded", "unknown"] },
                                "schema_version": { "type": "integer", "nullable": true },
                                "row_count_estimate": { "type": "integer", "nullable": true },
                                "last_write_at": { "type": "string", "format": "date-time", "nullable": true },
                                "error": { "type": "string" }
                            }
                        },
                        "clickhouse": {
                            "type": "object",
                            "required": ["state"],
                            "properties": {
                                "state": { "type": "string", "enum": ["healthy", "degraded", "not_configured", "unknown"] },
                                "url": { "type": "string", "nullable": true },
                                "row_count": { "type": "integer", "nullable": true },
                                "etl_lag_seconds": { "type": "number", "nullable": true },
                                "error": { "type": "string" }
                            }
                        },
                        "worker": {
                            "type": "object",
                            "required": ["state"],
                            "properties": {
                                "state": { "type": "string", "enum": ["running", "stopped", "unknown"] },
                                "uptime_seconds": { "type": "integer", "nullable": true },
                                "queue_depth": { "type": "integer", "nullable": true },
                                "error": { "type": "string" }
                            }
                        },
                        "webhook_target": {
                            "type": "object",
                            "required": ["configured"],
                            "properties": {
                                "configured": { "type": "boolean" },
                                "last_delivery_at": { "type": "string", "format": "date-time", "nullable": true },
                                "last_status": { "type": "string", "nullable": true },
                                "error": { "type": "string" }
                            }
                        },
                        "api_keys": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "required": ["provider", "configured"],
                                "properties": {
                                    "provider": { "type": "string" },
                                    "configured": { "type": "boolean" },
                                    "last_used_at": { "type": "string", "format": "date-time", "nullable": true }
                                }
                            }
                        },
                        "docker": {
                            "type": "object",
                            "required": ["present"],
                            "properties": {
                                "present": { "type": "boolean" },
                                "version": { "type": "string", "nullable": true },
                                "error": { "type": "string" }
                            }
                        }
                    }
                },
                "ClickHouseInstallAccepted": {
                    "type": "object",
                    "description": "Story 15.1 — 202 response from POST /v1/setup/clickhouse/install. `install_id` is a ULID the caller can use to subscribe to the SSE progress stream. MOCK in 15.1; real Docker calls land in Story 15.3.",
                    "required": ["install_id", "stream"],
                    "properties": {
                        "install_id": { "type": "string", "description": "ULID identifying the install." },
                        "stream": { "type": "string", "description": "Path to the SSE progress stream." }
                    }
                },
                "ClickHouseInstallEvent": {
                    "type": "object",
                    "description": "Story 15.1 — one frame of the SSE install stream. Step ordering: docker_detected → image_pulling → container_starting → provisioning_user → applying_migrations → running_parity_test → complete.",
                    "required": ["step", "progress", "log_line", "at"],
                    "properties": {
                        "step": { "type": "string", "enum": ["docker_detected", "image_pulling", "container_starting", "provisioning_user", "applying_migrations", "running_parity_test", "complete"] },
                        "progress": { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                        "log_line": { "type": "string" },
                        "at": { "type": "string", "format": "date-time" }
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
            "/v1/comparisons": {
                "get": {
                    "operationId": "comparisons",
                    "summary": "Phase 3 Story 0.8 — deterministic brand-vs-competitors comparison matrix (substrate for MCP `compare_brands`).",
                    "parameters": [
                        { "$ref": "#/components/parameters/ProjectHeader" },
                        { "name": "brands", "in": "query", "required": true, "schema": { "type": "string", "description": "Comma-separated; 2..=6 entries. First entry is the subject brand; remainder are competitors in caller-declared order." } },
                        { "name": "prompts", "in": "query", "schema": { "type": "string", "description": "Comma-separated prompt names; default = all prompts for the project." } },
                        { "name": "providers", "in": "query", "schema": { "type": "string", "description": "Comma-separated provider names; default = all providers." } },
                        { "name": "window", "in": "query", "schema": { "type": "string", "enum": ["1d", "7d", "30d"], "default": "7d" } }
                    ],
                    "responses": {
                        "200": {
                            "description": "OK",
                            "content": { "application/json": { "schema": { "$ref": "#/components/schemas/ComparisonsResponse" } } }
                        },
                        "400": {
                            "description": "Validation error (e.g. `brands` outside 2..=6, or unknown `window`).",
                            "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Error" } } }
                        },
                        "401": { "$ref": "#/components/responses/Unauthorized" }
                    }
                }
            },
            "/v1/healthz": {
                "get": {
                    "operationId": "healthz",
                    "summary": "Health probe",
                    "parameters": [
                        { "$ref": "#/components/parameters/ProjectHeader" }
                    ],
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
                        { "$ref": "#/components/parameters/ProjectHeader" },
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
                        { "$ref": "#/components/parameters/ProjectHeader" },
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
                        { "$ref": "#/components/parameters/ProjectHeader" },
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
                    "parameters": [
                        { "$ref": "#/components/parameters/ProjectHeader" }
                    ],
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
            "/v1/setup/status": {
                "get": {
                    "operationId": "setupStatus",
                    "summary": "Story 15.1 — synchronous status probe across Postgres, ClickHouse, worker, webhook target, API keys, and Docker. Always returns 200; individual sections report `state: \"unknown\"` on probe failure or timeout.",
                    "parameters": [
                        { "$ref": "#/components/parameters/ProjectHeader" }
                    ],
                    "responses": {
                        "200": {
                            "description": "OK",
                            "content": { "application/json": { "schema": { "$ref": "#/components/schemas/SetupStatus" } } }
                        },
                        "401": { "$ref": "#/components/responses/Unauthorized" }
                    }
                }
            },
            "/v1/setup/clickhouse/install": {
                "post": {
                    "operationId": "clickhouseInstall",
                    "summary": "Story 15.1 — kick off (MOCK) ClickHouse local-install state machine. Returns 202 with a ULID and an SSE stream URL. Real Docker calls land in Story 15.3.",
                    "parameters": [
                        { "$ref": "#/components/parameters/ProjectHeader" }
                    ],
                    "responses": {
                        "202": {
                            "description": "Accepted",
                            "content": { "application/json": { "schema": { "$ref": "#/components/schemas/ClickHouseInstallAccepted" } } }
                        },
                        "401": { "$ref": "#/components/responses/Unauthorized" }
                    }
                }
            },
            "/v1/setup/clickhouse/install-stream": {
                "get": {
                    "operationId": "clickhouseInstallStream",
                    "summary": "Story 15.1 — SSE stream of install progress events keyed by `id` (the ULID returned from POST /v1/setup/clickhouse/install). Closes when state reaches `complete` or `failed`.",
                    "parameters": [
                        { "$ref": "#/components/parameters/ProjectHeader" },
                        { "name": "id", "in": "query", "required": true, "schema": { "type": "string", "description": "Install ULID." } }
                    ],
                    "responses": {
                        "200": {
                            "description": "text/event-stream — each frame is a ClickHouseInstallEvent serialised as the SSE `data:` field.",
                            "content": { "text/event-stream": { "schema": { "$ref": "#/components/schemas/ClickHouseInstallEvent" } } }
                        },
                        "400": {
                            "description": "Malformed `id` (not a ULID).",
                            "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Error" } } }
                        },
                        "401": { "$ref": "#/components/responses/Unauthorized" },
                        "404": {
                            "description": "Unknown install `id` — POST /v1/setup/clickhouse/install first.",
                            "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Error" } } }
                        }
                    }
                }
            },
            "/v1/projects/{project_id}/events": {
                "get": {
                    "operationId": "subscribeEvents",
                    "summary": "Server-Sent Events stream of ARCH-17 lifecycle events for one project.",
                    "parameters": [
                        { "$ref": "#/components/parameters/ProjectHeader" },
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
