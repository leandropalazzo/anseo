# Anseo Open-Core Boundary

**Canonical definition per ADR-007 (2026-06-03). Both `docs/CONTRIBUTING.md` and `README.md` reference this file — edit here, not there.**

---

## MIT OSS Core (`opengeo` — public)

Everything needed to run a fully capable single-operator, multi-project Anseo deployment is MIT-licensed and lives in the public `opengeo` repo:

- Data collection, prompt execution, and provider adapters
- Mention and citation extraction
- GEO analytics (Postgres query layer; ClickHouse via opt-in plugin)
- Crawler ingest and audit heuristics
- GEO recommendations engine (all ten kinds, including LLM-assisted)
- Multi-project support (`ogeo project` verbs, `X-OpenGEO-Project` header, per-project secret keying)
- `ogeo serve` (Tier-1 single-binary turnkey: embedded Postgres, in-process worker, static dashboard, MCP HTTP/SSE)
- Plugin host and marketplace OSS surface (manifest, ed25519 TOFU signing, capability catalog)
- TypeScript and Python SDKs (generated from the wire-schema OpenAPI spec)
- All CLI commands, the REST `/v1` API, scheduling, webhooks, and the MCP server

## PRIVATE (`opengeo-internal` overlay only)

The following surfaces are Pro/cloud-only and never appear in the public repo:

- Pro crates: multi-org RLS isolation, RBAC (5-role), SSO/MFA, per-org KMS, Stripe billing, agency white-label/client portals, audit/DR
- Premium hallucination and brand-accuracy verdict surfaces
- Cloud infrastructure (`infra-cloud/`: Terraform, Kubernetes manifests, cloud-provider config)
- BMad planning/tooling (`_bmad/`, `_bmad-output/`, `.claude/skills/`)
- Internal docs (`docs-internal/`)

## Structural guarantee

OSS-builds-without-Pro is a **compile-time structural property**, not a vigilance exercise. No OSS crate in the public repo names a Pro crate — a missing Pro crate is a compile error, not a silent missing feature. The leak-check CI gate (Epic 38.2) is release-blocking.
