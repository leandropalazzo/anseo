import Link from "next/link";
import { ArrowRight } from "lucide-react";

import { Sparkline } from "@/components/charts/sparkline";
import { Card } from "@/components/ui/card";
import { DemoBadge } from "@/components/demo-badge";
import { EmptyState } from "@/components/ui/empty-state";
import { StatTile } from "@/components/ui/stat-tile";
import {
  fetchAnomalies,
  fetchBrands,
  fetchCitationSummary,
  fetchRecommendations,
  fetchOverviewTrendSeries,
  fetchKpiTrend,
  fetchRunSummary,
  fetchRuns,
  fetchTagSummary,
  fetchVisibilityOverall,
  type VisibilityMatrixCell,
  type AnomalyItem,
  type BrandItem,
  type CitationSummaryRow,
  type Recommendation,
  type RunListRow,
  type RunSummaryItem,
  type TagSummaryItem,
} from "@/lib/api";
import { Pill } from "@/components/ui/pill";
import { demoOrEmpty, IS_DEMO } from "@/lib/data-source";
import {
  isConcreteProviderId,
  resolveConcreteProviderId,
  type ConcreteProviderId,
} from "@/lib/provider-colors";

import { AnomalyTimeline } from "./_components/anomaly-timeline";
import {
  CitationStrip,
  type CitationStripRow,
} from "./_components/citation-strip";
import { CliParity } from "./_components/cli-parity";
import {
  CompetitorBars,
  type CompetitorBarRow,
} from "./_components/competitor-bars";
import { HeroStrip } from "./_components/hero-strip";
import {
  ProviderRanks,
  type ProviderRankRow,
} from "./_components/provider-ranks";
import {
  RecentRunsList,
  type RecentRunRow,
} from "./_components/recent-runs-list";
import { TopRecommendations } from "./_components/top-recommendations";

/** RFC3339 timestamp 7 days ago — the Overview KPI window. */
function sevenDaysAgo(): string {
  return new Date(Date.now() - 7 * 24 * 3600 * 1000).toISOString();
}

function arrayOrEmpty<T>(value: ReadonlyArray<T> | null | undefined): T[] {
  return Array.isArray(value) ? [...value] : [];
}

export default async function OverviewPage() {
  const since = sevenDaysAgo();

  const [runSummary, brands, anomalies, recentRuns, citationRows, recommendations, tagSummary, visibilityMatrix] =
    await Promise.all([
      tryFetchRunSummary(since),
      tryFetchBrands(),
      tryFetchAnomalies(),
      tryFetchRecentRuns(),
      tryFetchCitations(),
      tryFetchRecommendations(),
      tryFetchTagSummary(since),
      tryFetchVisibilityMatrix(),
    ]);

  // ── KPI derivations ─────────────────────────────────────────────────────
  // Run-summary rows are per-prompt rollups; aggregate across prompts.
  const totalRuns = runSummary.reduce((a, r) => a + r.run_count, 0);
  // Run-weighted mean success rate over prompts that report one.
  const withRate = runSummary.filter((r) => r.success_rate !== undefined);
  const ratedRuns = withRate.reduce((a, r) => a + r.run_count, 0);
  const successRate =
    ratedRuns > 0
      ? withRate.reduce((a, r) => a + (r.success_rate ?? 0) * r.run_count, 0) /
        ratedRuns
      : 0;
  // Run-weighted mean latency over prompts that report one.
  const withLatency = runSummary.filter((r) => r.avg_latency_ms !== undefined);
  const latencyRuns = withLatency.reduce((a, r) => a + r.run_count, 0);
  const avgLatencyMs =
    latencyRuns > 0
      ? withLatency.reduce(
          (a, r) => a + (r.avg_latency_ms ?? 0) * r.run_count,
          0,
        ) / latencyRuns
      : 0;
  // Distinct providers seen across all prompts this window.
  const distinctProviders = new Set(
    runSummary.flatMap((r) => r.providers),
  );

  // Primary brand drives the hero + brand-rank tile; fall back to the first
  // brand if none is flagged primary.
  const primaryBrand =
    brands.find((b) => b.is_primary) ?? brands[0];
  // The API returns `null` (not just absent) when a brand has no ranked runs
  // in the window; collapse to undefined so the `!== undefined` guards below
  // (and HeroStrip's) render an em-dash instead of crashing on null.toFixed().
  const primaryRank = primaryBrand?.avg_rank_7d ?? undefined;
  const primaryMentions = primaryBrand?.mention_count_7d ?? 0;
  const brandName = primaryBrand?.name ?? "—";

  // Overall presence rate: run-weighted average across all providers.
  // Computed after byProvider is built below — placeholder set after that block.

  // Real per-bucket sparkline series for the brand-rank tile, from
  // `/api/visibility/trend`. Use the most-active prompt this window (highest
  // run count) as a representative series. Tiles without a per-bucket series
  // (success rate, runs, latency) render the number alone — no synthesized trend.
  const trendPrompt = [...runSummary].sort(
    (a, b) => b.run_count - a.run_count,
  )[0]?.prompt;
  const rankSeries = trendPrompt
    ? (await fetchOverviewTrendSeries(trendPrompt, 7)).ranks
    : [];

  // Hourly project-wide KPI series (7d window) for the success/runs/latency
  // tile sparklines. Hourly so a single active day still yields a curve.
  const kpiTrend = await fetchKpiTrend(7 * 24);
  const runsSeries = kpiTrend.map((p) => p.run_count);
  const successSeries = kpiTrend.map((p) => p.success_rate * 100);
  const latencySeries = kpiTrend
    .map((p) => (p.avg_latency_ms ?? 0) / 1000)
    .filter((v) => v > 0);

  const failedRuns = Math.round(totalRuns * (1 - successRate));

  // Per-provider visibility from the overall visibility matrix — real
  // concrete-provider run counts, run-weighted avg rank, and presence rate.
  // Legacy OpenRouter-routed identities are normalized in the render layer.
  const byProvider = aggregateProviderRanks(visibilityMatrix);

  // Fall back to the run-summary base-provider keys only when the matrix is
  // empty (e.g. no mention data extracted yet), so the card still lists the
  // providers that produced runs.
  const fallbackProviders: ProviderRankRow[] = [];
  if (byProvider.length === 0) {
    const providerRuns = new Map<ConcreteProviderId, number>();
    for (const r of runSummary) {
      for (const p of r.providers) {
        if (isConcreteProviderId(p)) {
          providerRuns.set(p, (providerRuns.get(p) ?? 0) + r.run_count);
        }
      }
    }
    for (const [provider, count] of providerRuns) {
      fallbackProviders.push({
        provider,
        count,
        rank: primaryRank ?? 0,
        rate: successRate,
      });
    }
  }
  const providerRows = byProvider.length > 0 ? byProvider : fallbackProviders;

  // Run-weighted overall presence rate across all providers.
  const totalProviderRuns = providerRows.reduce((a, p) => a + p.count, 0);
  const overallPresenceRate =
    totalProviderRuns > 0
      ? providerRows.reduce((a, p) => a + p.rate * p.count, 0) / totalProviderRuns
      : undefined;

  // Hero provider count: distinct provider identities seen in the matrix, or
  // the run-summary's base providers when the matrix is empty.
  const heroProviderCount =
    providerRows.length > 0 ? providerRows.length : distinctProviders.size;

  // Competitor share-of-voice from brands' 7d mention counts.
  const competitorRows: CompetitorBarRow[] = brands.map((b) => ({
    name: b.name,
    mentions: b.mention_count_7d,
    isPrimary: b.is_primary,
  }));

  const hasKpis = totalRuns > 0 || brands.length > 0;

  return (
    <section data-testid="overview-page" className="flex flex-col gap-[16px]">
      <HeroStrip
        brandName={brandName}
        avgRank={primaryRank}
        presenceRate={overallPresenceRate}
        mentions={primaryMentions}
        successRate={successRate * 100}
        totalRuns={totalRuns}
        failedCount={failedRuns}
        providerCount={heroProviderCount}
      />

      {anomalies.isDemo && (
        <div className="flex justify-end">
          <DemoBadge />
        </div>
      )}
      {anomalies.isEmpty ? (
        <EmptyState
          title="No anomalies detected"
          message="window=7d · the detector found no rank, visibility, or citation shifts"
        />
      ) : (
        <AnomalyTimeline items={anomalies.data} />
      )}

      {hasKpis ? (
        <div className="grid grid-cols-4 gap-[12px]">
          <StatTile
            label="Brand avg rank"
            value={primaryRank !== undefined ? primaryRank.toFixed(2) : "—"}
            delta={`${primaryMentions} mentions · 7d`}
            deltaTone="neutral"
            mono
            sparkline={
              rankSeries.length > 1 ? (
                <Sparkline
                  points={rankSeries}
                  color="var(--accent)"
                  width={140}
                />
              ) : undefined
            }
          />
          <StatTile
            label="Success rate"
            value={`${(successRate * 100).toFixed(0)}%`}
            delta={failedRuns === 0 ? "all ok" : `${failedRuns} failed`}
            deltaTone={failedRuns === 0 ? "ok" : "warn"}
            mono
            sparkline={
              successSeries.length > 1 ? (
                <Sparkline points={successSeries} color="var(--ok)" width={140} />
              ) : undefined
            }
          />
          <StatTile
            label="Runs · last 7d"
            value={totalRuns}
            delta={`${distinctProviders.size} providers`}
            deltaTone="neutral"
            mono
            sparkline={
              runsSeries.length > 1 ? (
                <Sparkline points={runsSeries} color="var(--accent)" width={140} />
              ) : undefined
            }
          />
          <StatTile
            label="Avg latency"
            value={`${(avgLatencyMs / 1000).toFixed(2)}s`}
            delta={`${distinctProviders.size} providers`}
            deltaTone="neutral"
            mono
            sparkline={
              latencySeries.length > 1 ? (
                <Sparkline
                  points={latencySeries}
                  color="var(--warn)"
                  width={140}
                />
              ) : undefined
            }
          />
        </div>
      ) : (
        <EmptyState
          title="No runs yet"
          message="run a prompt (ogeo prompt run) to populate KPIs for the last 7 days"
        />
      )}

      <div className="grid grid-cols-[1.4fr_1fr] gap-[12px]">
        <Card
          title="Visibility by provider"
          eyebrow="last 7 days"
          action={
            <Link
              href="/visibility"
              className="inline-flex items-center gap-[5px] border border-[color:var(--border)] px-[8px] py-[4px] text-[length:var(--font-size-sm)] text-[color:var(--text-muted)] hover:text-[color:var(--text)]"
            >
              Open trend <ArrowRight size={11} strokeWidth={1.5} />
            </Link>
          }
        >
          {providerRows.length > 0 ? (
            <ProviderRanks
              providers={providerRows}
              brand={brandName}
              promptCount={runSummary.length}
            />
          ) : (
            <EmptyState
              title="No provider data"
              message="no runs recorded across providers in the last 7 days"
            />
          )}
        </Card>

        <Card
          title="Recent runs"
          eyebrow="live"
          padding={false}
          action={
            <Link
              href="/runs"
              className="inline-flex items-center gap-[5px] border border-[color:var(--border)] px-[8px] py-[4px] text-[length:var(--font-size-sm)] text-[color:var(--text-muted)] hover:text-[color:var(--text)]"
            >
              All runs <ArrowRight size={11} strokeWidth={1.5} />
            </Link>
          }
        >
          {recentRuns.length > 0 ? (
            <RecentRunsList runs={recentRuns} />
          ) : (
            <EmptyState
              title="No runs yet"
              message="ogeo prompt run · then refresh"
              className="m-[14px]"
            />
          )}
        </Card>
      </div>

      <div className="grid grid-cols-2 gap-[12px]">
        <Card
          title="Top citations"
          eyebrow="aggregated · last 7d"
          action={
            <Link
              href="/citations"
              className="inline-flex items-center gap-[5px] border border-[color:var(--border)] px-[8px] py-[4px] text-[length:var(--font-size-sm)] text-[color:var(--text-muted)] hover:text-[color:var(--text)]"
            >
              Explore <ArrowRight size={11} strokeWidth={1.5} />
            </Link>
          }
        >
          {citationRows.length > 0 ? (
            <CitationStrip rows={citationRows} />
          ) : (
            <EmptyState
              title="No citations yet"
              message="citations are extracted from provider responses on each run"
            />
          )}
        </Card>
        <Card
          title="Competitor share of voice"
          eyebrow={`brand: ${brandName.toLowerCase()}`}
          action={
            <Link
              href="/competitors"
              className="inline-flex items-center gap-[5px] border border-[color:var(--border)] px-[8px] py-[4px] text-[length:var(--font-size-sm)] text-[color:var(--text-muted)] hover:text-[color:var(--text)]"
            >
              Compare <ArrowRight size={11} strokeWidth={1.5} />
            </Link>
          }
        >
          {competitorRows.length > 0 ? (
            <CompetitorBars rows={competitorRows} />
          ) : (
            <EmptyState
              title="No tracked brands"
              message="add competitor brands in setup to compare share of voice"
            />
          )}
        </Card>
      </div>

      <Card
        title="Top recommendations"
        eyebrow="highest priority"
        action={
          <Link
            href="/recommendations"
            className="inline-flex items-center gap-[5px] border border-[color:var(--border)] px-[8px] py-[4px] text-[length:var(--font-size-sm)] text-[color:var(--text-muted)] hover:text-[color:var(--text)]"
          >
            All recommendations <ArrowRight size={11} strokeWidth={1.5} />
          </Link>
        }
      >
        <TopRecommendations items={recommendations} />
      </Card>

      <Card
        title="Summary by tag"
        eyebrow="last 7 days"
        action={
          <Link
            href="/prompts"
            className="inline-flex items-center gap-[5px] border border-[color:var(--border)] px-[8px] py-[4px] text-[length:var(--font-size-sm)] text-[color:var(--text-muted)] hover:text-[color:var(--text)]"
          >
            Manage prompts <ArrowRight size={11} strokeWidth={1.5} />
          </Link>
        }
      >
        {tagSummary.length > 0 ? (
          <div data-testid="tag-summary" className="flex flex-col">
            <div className="grid grid-cols-[1fr_80px_80px_90px_1fr] gap-[8px] border-b border-[color:var(--hairline)] pb-[6px] text-[length:var(--font-size-xs)] uppercase tracking-[0.06em] text-[color:var(--text-faint)]">
              <span>tag</span>
              <span className="text-right">prompts</span>
              <span className="text-right">runs</span>
              <span className="text-right">success</span>
              <span>providers</span>
            </div>
            {tagSummary.map((t) => (
              <div
                key={t.tag}
                data-testid={`tag-row-${t.tag}`}
                className="grid grid-cols-[1fr_80px_80px_90px_1fr] items-center gap-[8px] border-b border-[color:var(--hairline)] py-[8px] text-[length:var(--font-size-sm)] text-[color:var(--text)]"
              >
                <span>
                  <Pill mono tone={t.tag === "AUTO" ? "info" : undefined}>
                    {t.tag}
                  </Pill>
                </span>
                <span className="text-right font-[family-name:var(--font-mono)]">
                  {t.prompt_count}
                </span>
                <span className="text-right font-[family-name:var(--font-mono)]">
                  {t.run_count}
                </span>
                <span className="text-right font-[family-name:var(--font-mono)]">
                  {t.success_rate != null
                    ? `${(t.success_rate * 100).toFixed(0)}%`
                    : "—"}
                </span>
                <span className="overflow-hidden text-ellipsis whitespace-nowrap text-[length:var(--font-size-xs)] text-[color:var(--text-muted)]">
                  {t.providers.length > 0 ? t.providers.join(", ") : "—"}
                </span>
              </div>
            ))}
          </div>
        ) : (
          <EmptyState
            title="No tagged prompts yet"
            message="add tags to prompts (or generate them with AI) to see per-tag rollups"
          />
        )}
      </Card>

      <CliParity />
    </section>
  );
}

async function tryFetchVisibilityMatrix(): Promise<VisibilityMatrixCell[]> {
  try {
    const r = await fetchVisibilityOverall(7);
    return arrayOrEmpty(r.matrix);
  } catch {
    return [];
  }
}

/** Collapses the (prompt × provider) matrix into one row per provider:
 *  summed run counts, run-weighted avg rank, and run-weighted presence rate. */
function aggregateProviderRanks(
  matrix: ReadonlyArray<VisibilityMatrixCell>,
): ProviderRankRow[] {
  type Acc = {
    runs: number;
    rankWeight: number;
    rankSum: number;
    presenceSum: number;
  };
  const byProvider = new Map<string, Acc>();
  for (const cell of matrix) {
    const providerKey = resolveConcreteProviderId(cell.provider) ?? cell.provider;
    const acc =
      byProvider.get(providerKey) ??
      { runs: 0, rankWeight: 0, rankSum: 0, presenceSum: 0 };
    acc.runs += cell.run_count;
    acc.presenceSum += cell.presence_rate * cell.run_count;
    if (cell.avg_rank !== null) {
      acc.rankWeight += cell.run_count;
      acc.rankSum += cell.avg_rank * cell.run_count;
    }
    byProvider.set(providerKey, acc);
  }
  return [...byProvider.entries()]
    .map(([provider, a]) => ({
      provider,
      count: a.runs,
      rank: a.rankWeight > 0 ? a.rankSum / a.rankWeight : 0,
      rate: a.runs > 0 ? a.presenceSum / a.runs : 0,
    }))
    .sort((x, y) => x.rank - y.rank);
}

async function tryFetchRunSummary(since: string): Promise<RunSummaryItem[]> {
  try {
    const r = await fetchRunSummary(since);
    return arrayOrEmpty(r.items);
  } catch {
    return [];
  }
}

async function tryFetchTagSummary(since: string): Promise<TagSummaryItem[]> {
  try {
    const r = await fetchTagSummary(since);
    return arrayOrEmpty(r.items);
  } catch {
    return [];
  }
}

async function tryFetchBrands(): Promise<BrandItem[]> {
  try {
    const r = await fetchBrands();
    return arrayOrEmpty(r.items);
  } catch {
    return [];
  }
}

async function tryFetchAnomalies() {
  let items: AnomalyItem[] = [];
  try {
    items = await fetchAnomalies("7d");
  } catch {
    items = [];
  }
  // Demo mode shows mock data behind a badge; otherwise empty → EmptyState.
  return demoOrEmpty<AnomalyItem>(items, () =>
    IS_DEMO ? demoAnomalies() : [],
  );
}

async function tryFetchRecentRuns(): Promise<RecentRunRow[]> {
  try {
    const r = await fetchRuns({ limit: 8 });
    return arrayOrEmpty(r.runs).map(toRecentRunRow);
  } catch {
    return [];
  }
}

function toRecentRunRow(r: RunListRow): RecentRunRow {
  return {
    id: r.id,
    prompt_name: r.prompt_name,
    provider: r.provider,
    started_at: r.started_at,
    status: r.status,
    error_kind: r.error_kind,
  };
}

async function tryFetchCitations(): Promise<CitationStripRow[]> {
  try {
    const r = await fetchCitationSummary(12);
    return arrayOrEmpty(r.domains).map((d: CitationSummaryRow) => ({
      domain: d.domain,
      frequency: d.frequency,
    }));
  } catch {
    return [];
  }
}

async function tryFetchRecommendations(): Promise<Recommendation[]> {
  try {
    const r = await fetchRecommendations({ limit: 20 });
    return arrayOrEmpty(r.items);
  } catch {
    return [];
  }
}

/** Minimal demo anomaly set, only reached under `IS_DEMO`. */
function demoAnomalies(): AnomalyItem[] {
  const now = Date.now();
  return [
    {
      id: "demo_anom_1",
      kind: "rank_swap",
      prompt: "vector-db",
      provider: "openai",
      detected_at: new Date(now - 2 * 3600 * 1000).toISOString(),
      severity: "medium",
      delta: 0.4,
      window_days: 7,
      details: { note: "demo" },
    },
    {
      id: "demo_anom_2",
      kind: "visibility_drop",
      prompt: "observability",
      provider: "anthropic",
      detected_at: new Date(now - 26 * 3600 * 1000).toISOString(),
      severity: "low",
      delta: 0.12,
      window_days: 7,
      details: { note: "demo" },
    },
  ];
}
