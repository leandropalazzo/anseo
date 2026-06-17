# Canonical GEO Prompt Suite

The canonical GEO prompt suite is the shared benchmark vocabulary for externally
instrumented runs. It gives operators a stable list of prompt slugs to reuse so
contributions from different projects land in the same cohort instead of passing
each other in the dark.

The source of truth lives in the benchmark crate at:

- `crates/benchmark/data/canonical_geo_prompt_suite.v1.json`

That JSON artifact is versioned, validated in tests, and is the file Story
`40.5` will consume directly for the CLI/API/MCP suite surfaces.

## Change-control rules

The suite follows a conservative compatibility contract:

1. Within a suite version, changes are additive only.
2. Existing slugs may be marked deprecated, but they are never repurposed.
3. Removing or semantically rewriting a slug requires a new `suite_id`.
4. Each entry must keep stable `slug`, `version`, category, and cohort intent.
5. The suite's `terms_version` must match the benchmark contribution terms
   version pinned by the redactor.

This keeps old benchmark cohorts interpretable even as new prompt families are
added.

## Legal / compliance note

The suite text is intentionally limited to benchmark-owned reference material so
it can be reviewed against the finalized benchmark terms (`v1-2026-05-28`).
Operators remain free to run their own private prompt wording locally, but only
runs aligned to these canonical slugs join the shared public cohorts.

Story `39.3` still requires the legal/compliance signoff itself to be recorded
before the story flips from `review` to `done`. This document captures the
artifact and its intended legal boundary; it does not pretend that an external
approval record already exists.

## Initial suite (`geo-v1`)

| Slug | Category | Cohort | Description |
|---|---|---|---|
| `geo-v1/best-vector-db` | `platform-selection` | `geo-v1:platform-selection` | Category-comparison query for vector database selection. |
| `geo-v1/best-rag-platform` | `platform-selection` | `geo-v1:platform-selection` | Category-comparison query for full RAG platform evaluation. |
| `geo-v1/llm-observability-tools` | `observability` | `geo-v1:observability` | Tooling-comparison query for LLM observability products. |
| `geo-v1/ai-search-visibility-platforms` | `brand-visibility` | `geo-v1:brand-visibility` | Category query for AI-search visibility and citation monitoring platforms. |
| `geo-v1/best-enterprise-chatbot-platform` | `application-platform` | `geo-v1:application-platform` | Category query for enterprise chatbot and assistant platforms. |
| `geo-v1/agent-frameworks` | `developer-frameworks` | `geo-v1:developer-frameworks` | Framework-comparison query for agent orchestration libraries. |
| `geo-v1/best-ai-evaluation-tools` | `evaluation` | `geo-v1:evaluation` | Tooling-comparison query for LLM evaluation and regression testing products. |
| `geo-v1/customer-support-ai` | `use-case-solutions` | `geo-v1:use-case-solutions` | Solution-comparison query for customer-support AI products. |

The JSON artifact also carries the canonical prompt template for each slug so
future API/CLI/MCP surfaces can expose the suite without inventing new data
models.
