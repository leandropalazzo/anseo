/**
 * UX-E mock data — additions for Ops surfaces (Prompts, Alerts, MCP,
 * Settings, Onboarding). UX-C `lib/mock.ts` + UX-D `lib/mock-analytics.ts`
 * remain read-only; this module owns only what UX-E introduces.
 *
 * Shapes mirror the eventual API responses so the swap from mock to live
 * is a one-line change per call site.
 *
 * Source prototype: _bmad-output/planning-artifacts/ux-redesign-2026-05-29/project/src/screens-ops.jsx
 */

import type { ProviderId } from "@/lib/provider-colors";

// ─── Alerts: rules table ─────────────────────────────────────────────────────

export type AlertRuleStatus = "armed" | "muted";

export interface AlertRule {
  name: string;
  /** Human-readable condition expression. */
  on: string;
  /** Prompt name or `*` for all. */
  target: string;
  channels: ReadonlyArray<string>;
  status: AlertRuleStatus;
  /** Number of times the rule fired in the last 7 days. */
  fires: number;
}

export const ALERT_RULES: ReadonlyArray<AlertRule> = [
  { name: "ranking-drop",      on: "brand_rank > 5",            target: "vector-db", channels: ["slack", "email"],       status: "armed", fires: 4 },
  { name: "presence-floor",    on: "presence_rate < 70%",       target: "*",         channels: ["slack"],                status: "armed", fires: 1 },
  { name: "new-competitor",    on: "new_brand_share > 5%",      target: "*",         channels: ["email", "webhook"],     status: "armed", fires: 1 },
  { name: "provider-degraded", on: "failed_runs > 3 in 1h",     target: "*",         channels: ["pagerduty"],            status: "armed", fires: 0 },
  { name: "citation-spike",    on: "domain_freq > 2x_baseline", target: "vector-db", channels: ["slack"],                status: "muted", fires: 0 },
  { name: "weekly-digest",     on: "every monday 09:00 UTC",    target: "*",         channels: ["email"],                status: "armed", fires: 0 },
];

// ─── Alerts: upcoming runs (schedule grid sidebar) ───────────────────────────

export interface UpcomingRun {
  in: string;
  prompt: string;
  provider: ProviderId;
}

export const UPCOMING_RUNS: ReadonlyArray<UpcomingRun> = [
  { in: "00:14", prompt: "ai-search-eval", provider: "openai" },
  { in: "00:47", prompt: "vector-db",      provider: "anthropic" },
  { in: "01:14", prompt: "edge-runtime",   provider: "gemini" },
  { in: "01:47", prompt: "vector-db",      provider: "perplexity" },
  { in: "02:14", prompt: "observability",  provider: "openai" },
  { in: "03:00", prompt: "auth-saas",      provider: "anthropic" },
];

// ─── Settings: providers ─────────────────────────────────────────────────────

export type ProviderConnectStatus = "connected" | "disconnected";

export interface ProviderSettingsRow {
  id: ProviderId;
  status: ProviderConnectStatus;
  model: string;
  budget: string;
  store: string;
  latency: string;
}

export const PROVIDER_SETTINGS: ReadonlyArray<ProviderSettingsRow> = [
  { id: "openai",     status: "connected",    model: "gpt-5-turbo",     budget: "$240 / mo", store: "keychain", latency: "1.1s p50" },
  { id: "anthropic",  status: "connected",    model: "claude-opus-4.1", budget: "$310 / mo", store: "keychain", latency: "1.4s p50" },
  { id: "gemini",     status: "connected",    model: "gemini-2.5-pro",  budget: "$90 / mo",  store: "env",      latency: "0.9s p50" },
  { id: "perplexity", status: "disconnected", model: "sonar-large",     budget: "—",         store: "—",        latency: "—" },
];

// ─── Settings: extractors ────────────────────────────────────────────────────

export interface ExtractorRow {
  name: string;
  enabled: boolean;
  detail: string;
}

export const EXTRACTORS: ReadonlyArray<ExtractorRow> = [
  { name: "mention.list-detect",      enabled: true,  detail: "Detects numbered/bulleted brand lists; ranks 1..n." },
  { name: "mention.span-match",       enabled: true,  detail: "Exact + fuzzy brand match within ±32 token spans." },
  { name: "citation.url-extract",     enabled: true,  detail: "URL + domain extraction; deduped per response." },
  { name: "citation.source-classify", enabled: true,  detail: "Tags: forum / docs / video / blog / paper / code / social." },
  { name: "sentiment.brand",          enabled: true,  detail: "Per-brand mention sentiment (positive / neutral / negative + score)." },
  { name: "claim.extract",            enabled: true,  detail: "Extracts factual brand claims for accuracy / hallucination checks." },
  { name: "embedding.sem-search",     enabled: false, detail: "Embed responses for semantic search (Phase 2)." },
];

// ─── Settings: team ──────────────────────────────────────────────────────────

export interface TeamMember {
  name: string;
  email: string;
  role: "Owner" | "Admin" | "Editor" | "Viewer";
  last: string;
}

export const TEAM_MEMBERS: ReadonlyArray<TeamMember> = [
  { name: "Diego Alvarado", email: "diego@pinecone.io", role: "Owner",  last: "now" },
  { name: "Mira Sato",      email: "mira@pinecone.io",  role: "Admin",  last: "12m ago" },
  { name: "Kofi Asare",     email: "kofi@pinecone.io",  role: "Editor", last: "1h ago" },
  { name: "Lena Volkova",   email: "lena@pinecone.io",  role: "Viewer", last: "1d ago" },
];

// ─── Settings: deploy cluster health ─────────────────────────────────────────
// (Removed in Story 46.3 — the Deployment settings section now reads api/worker
// liveness live from GET /v1/serve/status; remaining services are labeled as
// having no live probe yet rather than mocked here.)

// ─── Onboarding: first-run log ───────────────────────────────────────────────

export const FIRST_RUN_LOG: ReadonlyArray<string> = [
  "→ loading prompts.yaml",
  "→ resolving secrets (4 providers)",
  "→ openai     · gpt-5-turbo · 1834ms · rank 1 · 6 citations",
  "→ anthropic  · claude-opus · 1521ms · rank 1 · 4 citations",
  "→ gemini     · gemini-2.5  · 1102ms · rank 1 · 5 citations",
  "→ perplexity · sonar-large · 982ms  · rank 1 · 8 citations",
  "✓ persisted 4 runs · 23 mentions · 18 citations",
];

// ─── Onboarding: defaults ────────────────────────────────────────────────────

export const ONBOARDED_FLAG = "opengeo:onboarded";

export const DEFAULT_BRAND = "Pinecone";
export const DEFAULT_BRAND_ALIASES = "pinecone, pinecone.io, pc-vector";
export const DEFAULT_COMPETITORS: ReadonlyArray<string> = [
  "qdrant",
  "weaviate",
  "milvus",
  "chroma",
  "lancedb",
  "turbopuffer",
];

export const DEFAULT_ALERT_TOGGLES: ReadonlyArray<{
  label: string;
  on: boolean;
}> = [
  { label: "Ranking drops > 2 positions",         on: true },
  { label: "Brand presence falls below 70%",      on: true },
  { label: "New competitor appears in 5+ runs",   on: true },
  { label: "Weekly digest, Mondays 9am UTC",      on: false },
];
