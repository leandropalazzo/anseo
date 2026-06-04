// Barrel re-export for the OpenGEO API client.
//
// The client was split into per-feature modules under `lib/api/` (Story 30-1).
// This file re-exports them all so every existing `@/lib/api` import keeps
// working unchanged. Add new fetchers to the relevant feature module, not here.

export * from "./api/_client";
export * from "./api/runs";
export * from "./api/citations";
export * from "./api/visibility";
export * from "./api/sentiment";
export * from "./api/crawlers";
export * from "./api/audit";
export * from "./api/hallucination";
export * from "./api/analytics";
export * from "./api/competitors";
export * from "./api/anomalies";
export * from "./api/alerts";
export * from "./api/overview";
export * from "./api/recommendations";
export * from "./api/mcp";
export * from "./api/setup";
export * from "./api/marketplace";
export * from "./api/prompts";
export * from "./api/projects";
