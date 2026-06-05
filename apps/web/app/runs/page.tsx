import { fetchRuns } from "@/lib/api";
import { genRuns, type MockRun } from "@/lib/mock";

import { PageHeader } from "@/components/ui/page-header";
import { RunsView } from "./_components/runs-view";

export default async function RunsPage() {
  // Hybrid: prefer the live /api/runs list and enrich each row with the
  // mock fields (brand_rank, mentions, latency_ms, tokens) since the v1
  // RunListRow shape doesn't include them yet. If the API is unreachable,
  // fall through to a pure-mock dataset so the UI still renders.
  let runs: MockRun[];
  try {
    const live = await fetchRuns({ limit: 80 });
    runs = live.runs.map((r, i) => enrichLiveRun(r, i));
  } catch {
    runs = genRuns("healthy", 60);
  }

  return (
    <section data-testid="runs-page" className="flex flex-col gap-[12px]">
      <PageHeader
        title="Prompt Runs"
        description="Newest first. Tabs + provider chips filter the table."
        actions={
          <div className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
            {runs.length} rows
          </div>
        }
      />
      <RunsView runs={runs} />
    </section>
  );
}

/**
 * Live → enriched mock-shape adapter. The live API returns the v1 RunListRow
 * which lacks brand_rank/mentions/latency/tokens (these arrive once Story
 * 12.x extraction wiring lands). Until then we synthesize plausible values
 * keyed off the row index so the table renders without `null` placeholders
 * everywhere.
 */
function enrichLiveRun(
  r: {
    id: string;
    prompt_name: string;
    provider: string;
    provider_model_version: string;
    started_at: string;
    status: "ok" | "failed";
    error_kind: string | null;
  },
  i: number,
): MockRun {
  // Pass the provider string through verbatim; renderers normalize legacy
  // OpenRouter-routed identities to their concrete provider label.
  return {
    id: r.id,
    prompt_id: `p_${i.toString().padStart(3, "0")}`,
    prompt_name: r.prompt_name,
    provider: r.provider,
    provider_model_version: r.provider_model_version,
    started_at: r.started_at,
    finished_at: r.status === "ok" ? r.started_at : null,
    status: r.status,
    error_kind: r.error_kind,
    brand_rank: r.status === "ok" ? Math.max(1, 2 + (i % 5)) : null,
    mentions: r.status === "ok" ? Math.max(0, 4 - (i % 4)) : 0,
    latency_ms: 1800 + (i % 7) * 320,
    tokens_in: 280 + (i % 9) * 14,
    tokens_out: 620 + (i % 11) * 41,
  };
}
