import { readdirSync, readFileSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join, relative, sep } from "node:path";
import { describe, expect, it } from "vitest";

// ─── Silent-mock guard (Story 30-1) ─────────────────────────────────────────
//
// Mock data may only reach a rendered surface through the demo-data contract
// (`IS_DEMO`, see lib/data-source.ts). This test walks every source file under
// `app/**` and asserts none imports from `@/lib/mock*` UNLESS the same file
// also references `IS_DEMO` (i.e. gates the mock behind demo mode).
//
// The current tree predates that contract, so the violating files are
// explicitly allow-listed in KNOWN_MOCK_DEBT with the owning follow-up story.
// New violations (a file not on the list) fail the test — that's the point.

const here = dirname(fileURLToPath(import.meta.url));
const webRoot = join(here, "..");
const appRoot = join(webRoot, "app");

/** Matches `from "@/lib/mock"`, `@/lib/mock-analytics`, `@/lib/mock-ops`, … */
const MOCK_IMPORT_RE = /from\s+["']@\/lib\/mock[\w-]*["']/;

// Files that import a mock module WITHOUT gating it behind IS_DEMO. These are
// pre-30-1 debt; each is paid down by a downstream story which removes the
// import (or wraps it in the demo-data contract).
// TODO(30-2/30-3/30-4/30-5/30-7): retire entries as each surface goes live.
const KNOWN_MOCK_DEBT: ReadonlyArray<string> = [
  "app/(onboarding)/_components/step-brand.tsx",
  "app/(onboarding)/_components/step-first-run.tsx",
  "app/(onboarding)/_components/step-schedule-alerts.tsx",
  "app/(onboarding)/onboarding/page.tsx",
  "app/prompts/_components/rank-trend-mini.tsx",
  "app/prompts/_components/schedule-editor.tsx",
  "app/prompts/_components/yaml-editor.tsx",
  "app/runs/_components/runs-table.tsx",
  "app/runs/_components/runs-view.tsx",
  "app/runs/page.tsx",
  "app/settings/_components/extractors.tsx",
  "app/settings/page.tsx",
];

function walk(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) {
      out.push(...walk(full));
    } else if (/\.(ts|tsx)$/.test(entry)) {
      out.push(full);
    }
  }
  return out;
}

/** Repo-relative POSIX path, e.g. `app/runs/page.tsx`. */
function rel(full: string): string {
  return relative(webRoot, full).split(sep).join("/");
}

describe("app/** mock-import guard", () => {
  const files = walk(appRoot);

  it("finds source files to scan", () => {
    expect(files.length).toBeGreaterThan(0);
  });

  it("no file imports @/lib/mock* without gating behind IS_DEMO (except known debt)", () => {
    const debt = new Set(KNOWN_MOCK_DEBT);
    const violations: string[] = [];

    for (const full of files) {
      const src = readFileSync(full, "utf8");
      if (!MOCK_IMPORT_RE.test(src)) continue;
      // A mock import is allowed when the file gates it behind the demo
      // contract (references IS_DEMO).
      if (src.includes("IS_DEMO")) continue;
      const path = rel(full);
      if (debt.has(path)) continue;
      violations.push(path);
    }

    expect(
      violations,
      `New mock-import violation(s). Gate the mock behind IS_DEMO (lib/data-source.ts) ` +
        `or, if intentional debt, add to KNOWN_MOCK_DEBT with a TODO(30-x):\n` +
        violations.map((v) => `  - ${v}`).join("\n"),
    ).toEqual([]);
  });

  it("KNOWN_MOCK_DEBT has no stale entries (every listed file still violates)", () => {
    const stale: string[] = [];
    for (const path of KNOWN_MOCK_DEBT) {
      const full = join(webRoot, path);
      let src: string;
      try {
        src = readFileSync(full, "utf8");
      } catch {
        stale.push(`${path} (file missing)`);
        continue;
      }
      const gated = !MOCK_IMPORT_RE.test(src) || src.includes("IS_DEMO");
      if (gated) stale.push(`${path} (no longer violates — remove from list)`);
    }
    expect(stale, stale.join("\n")).toEqual([]);
  });
});

// ─── Story 41.3 AC7 — no Epic-17 mock plugin catalog remains ─────────────────
//
// Epic 17 (17.5/17.7/17.8) shipped the marketplace + CLI against a hardcoded
// mock plugin catalog (`done(mock)`). Story 41.3 drops it for the live registry.
// This test enforces that NONE of the known Epic-17 mock plugin ids/names linger
// anywhere under apps/web (and sdks/, if present) — replacing the prior
// code-inspection-only check. The scan excludes this test file (which must name
// the forbidden strings) and node_modules / build output.

/** The Epic-17 mock catalog identifiers that must no longer appear in source. */
const EPIC17_MOCK_IDS: ReadonlyArray<string> = [
  "opengeo/serp-enrichment",
  "community/markdown-export",
  "opengeo/clickhouse-window",
  "SERP Enrichment",
  "MARKETPLACE_MOCK",
  "marketplace-mock",
];

function walkAll(dir: string): string[] {
  const out: string[] = [];
  let entries: string[];
  try {
    entries = readdirSync(dir);
  } catch {
    return out;
  }
  for (const entry of entries) {
    if (entry === "node_modules" || entry === ".next" || entry === "dist") {
      continue;
    }
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) {
      out.push(...walkAll(full));
    } else if (/\.(ts|tsx|js|jsx|mjs)$/.test(entry)) {
      out.push(full);
    }
  }
  return out;
}

describe("Story 41.3 AC7 — mock plugin catalog is gone", () => {
  // apps/web is `<repo>/apps/web`; sdks (if it exists) is `<repo>/sdks`.
  const repoRoot = join(webRoot, "..", "..");
  const scanRoots = [join(repoRoot, "apps", "web"), join(repoRoot, "sdks")];
  const selfPath = fileURLToPath(import.meta.url);

  it("no Epic-17 mock plugin id/name appears anywhere under apps/web or sdks", () => {
    const offenders: string[] = [];
    for (const root of scanRoots) {
      for (const full of walkAll(root)) {
        if (full === selfPath) continue;
        const src = readFileSync(full, "utf8");
        for (const id of EPIC17_MOCK_IDS) {
          if (src.includes(id)) {
            offenders.push(`${relative(repoRoot, full)} contains "${id}"`);
          }
        }
      }
    }
    expect(
      offenders,
      `Epic-17 mock plugin data still present — drop it for the live registry:\n` +
        offenders.map((o) => `  - ${o}`).join("\n"),
    ).toEqual([]);
  });
});
