# OpenGEO

OpenGEO is the local, self-hosted observability loop for AI search visibility.

```bash
# Phase 1 local loop
ogeo init
ogeo login openai
ogeo login anthropic
ogeo prompt run
ogeo report generate --format markdown
```

Phase 1 ships the closed local path:

```text
YAML config -> CLI prompt runs -> provider calls -> extraction -> PostgreSQL persistence -> Docker Compose
```

This repository is a Cargo workspace containing the OpenGEO core, providers, extractors, storage, CLI, API server, MCP server, and worker. The OpenGEO web dashboard is developed separately and is not included here.

## Layout

```text
apps/
  api/      Axum API binary
  worker/   background worker binary
  cli/      `ogeo` CLI binary
  mcp/      MCP server binary
crates/
  core/         shared core contracts
  providers/    provider adapters
  extractors/   mention and citation extraction
  storage/      PostgreSQL persistence
  analytics/    analytics query layer
  scheduler/    Phase 2 scheduler stub
  plugin-host/  Phase 3 plugin host stub
  wire-schema/  shared API/schema DTOs
infra/
  docker/
  k8s/
  terraform/
```

## Toolchain

- Rust: `1.95.0`, edition 2021, pinned in `rust-toolchain.toml`

## Verification

```bash
cargo build
cargo test
cargo metadata --no-deps
```

## CLI Error Mapping

OpenGEO CLI commands return structured exit codes per PRD §11.4. CI integrations can rely on these being stable within a major version.

| Code | Meaning |
|------|---------|
| 0    | Success |
| 1    | Visibility check failed (Ranking exceeded threshold; FR-15) |
| 2    | Provider error (see Provider Error Taxonomy below) |
| 64   | Config error (malformed YAML, missing required field, unknown Provider/model) |
| 65   | Data error (corrupted persisted data, schema-version mismatch) |
| 66   | Auth/permission error (Phase 4 RBAC denial; Phase 2+ invalid API token) |
| 70   | Internal error (uncaught panic, bug) |

When a CLI command exits with code `2`, the structured output (and the `error_kind` field on JSON-format reports) carries one of the following stable values per PRD §11.5:

- `provider_unauthorized` — 401/403 from Provider (bad/missing API key)
- `provider_rate_limited` — 429 from Provider
- `provider_timeout` — request exceeded configured timeout
- `provider_5xx` — Provider returned 5xx
- `provider_invalid_response` — response failed schema/parse expectations
- `network_error` — DNS / TCP / TLS failure reaching Provider

Both contracts are defined in `crates/core/src/error.rs`.

## Privacy Posture

Phase 1 is localhost-first. Provider keys stay local, raw responses stay in the user's deployment, and OpenGEO sends no telemetry to OpenGEO-controlled services unless a later explicit opt-in feature is implemented.

## License

MIT — see [LICENSE](./LICENSE).
