import {
  fetchDeclaredPrompts,
  fetchVisibilityOverall,
  fetchVisibilityTrend,
  type VisibilityMatrixCell,
  type VisibilityPoint,
} from "@/lib/api";
import { IS_DEMO } from "@/lib/data-source";

import { PageHeader } from "@/components/ui/page-header";
import type { PromptOption } from "./_components/prompt-picker";
import type { TrendPoint } from "./_components/trend-chart";
import { VisibilityTabs } from "./_components/visibility-tabs";

interface SearchParams {
  prompt?: string;
  days?: string;
}

function pickDays(raw: string | undefined): 1 | 7 | 30 {
  const n = Number(raw);
  if (n === 1 || n === 7 || n === 30) return n;
  return 30;
}

/** Demo-only prompt list + trend generator. Imported lazily so the mock
 *  module never reaches the bundle/render path unless `OGEO_DEMO=1`. */
async function demoVisibility(
  promptName: string,
  days: 1 | 7 | 30,
): Promise<{ prompts: PromptOption[]; byProvider: Record<string, TrendPoint[]> }> {
  const { PROMPTS, PROVIDERS, genTrend } = await import("@/lib/mock");
  const prompts: PromptOption[] = PROMPTS.map((p) => ({
    id: p.id,
    name: p.name,
    text: p.text,
  }));
  const prompt = PROMPTS.find((p) => p.name === promptName) ?? PROMPTS[0]!;
  const byProvider: Record<string, TrendPoint[]> = {};
  for (const provider of PROVIDERS) {
    byProvider[provider] = genTrend("healthy", provider, prompt, days).map((pt) => ({
      bucket_start: pt.bucket_start,
      provider: pt.provider,
      avg_rank: pt.avg_rank,
      presence_rate: pt.presence_rate,
    }));
  }
  return { prompts, byProvider };
}

export default async function VisibilityPage({
  searchParams,
}: {
  searchParams: Promise<SearchParams>;
}) {
  const sp = await searchParams;
  const days = pickDays(sp.days);

  // Derive the prompt list from the live runs (declared prompt names). The
  // AddScheduleSheet uses the same `fetchDeclaredPrompts` source (runs.ts).
  let declared: string[] = [];
  try {
    declared = await fetchDeclaredPrompts();
  } catch {
    declared = [];
  }
  let prompts: PromptOption[] = declared.map((name) => ({ id: name, name }));

  // Selected prompt: honour `?prompt=`, else first declared.
  const selectedName = sp.prompt ?? prompts[0]?.name ?? "";

  // Live trend for the selected prompt, bucketed per provider.
  let byProvider: Record<string, TrendPoint[]> = {};
  let hasLive = false;
  if (selectedName) {
    try {
      const r = await fetchVisibilityTrend(selectedName, days);
      for (const p of r.points) {
        const arr = byProvider[p.provider] ?? [];
        arr.push({
          bucket_start: p.bucket_start,
          provider: p.provider,
          avg_rank: p.avg_rank ?? 0,
          presence_rate: p.presence_rate,
        });
        byProvider[p.provider] = arr;
      }
      hasLive = r.points.length > 0;
    } catch {
      byProvider = {};
      hasLive = false;
    }
  }

  // Demo contract: live trend present → render it. Empty + demo mode → mock
  // (with a visible <DemoBadge/>). Empty + not demo → <EmptyState/>.
  let isDemo = false;
  if (!hasLive && IS_DEMO) {
    const demo = await demoVisibility(selectedName, days);
    prompts = demo.prompts;
    byProvider = demo.byProvider;
    isDemo = true;
  }
  const isEmpty = !hasLive && !isDemo;
  const selected: PromptOption =
    prompts.find((p) => p.name === selectedName) ??
    prompts[0] ?? { id: "", name: "" };

  // Overall (all-prompts) data — independent of the per-prompt selection.
  let overallBrand = "";
  let overallMatrix: VisibilityMatrixCell[] = [];
  let overallTrend: VisibilityPoint[] = [];
  let overallWindow = 30;
  try {
    const o = await fetchVisibilityOverall(30);
    overallBrand = o.brand;
    overallMatrix = o.matrix;
    overallTrend = o.trend;
    overallWindow = o.window_days;
  } catch {
    overallMatrix = [];
    overallTrend = [];
  }

  return (
    <section data-testid="visibility-page" className="space-y-[12px]">
      <PageHeader
        title="Visibility"
        description="Per-prompt trend and an overall, all-prompts visibility matrix."
      />
      <VisibilityTabs
        byPrompt={{
          prompts,
          liveByProvider: byProvider,
          initialPrompt: selected,
          initialDays: days,
          isDemo,
          isEmpty,
        }}
        overall={{
          brand: overallBrand,
          windowDays: overallWindow,
          matrix: overallMatrix,
          trend: overallTrend,
        }}
      />
    </section>
  );
}
