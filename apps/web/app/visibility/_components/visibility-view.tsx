"use client";

import { useMemo, useState } from "react";

import { DemoBadge } from "@/components/demo-badge";
import { EmptyState } from "@/components/ui/empty-state";
import { Card } from "@/components/ui/card";
import { Pill } from "@/components/ui/pill";
import { ProviderDot } from "@/components/ui/provider-dot";
import { SegControl } from "@/components/ui/seg-control";
import { resolveProviderIdentity } from "@/lib/provider-colors";

import {
  DeltaLeaderboard,
  type DeltaLeaderboardRow,
} from "./delta-leaderboard";
import { PromptPicker, type PromptOption } from "./prompt-picker";
import { RankHeatmap, type HeatmapInputRow } from "./rank-heatmap";
import {
  TrendChart,
  type TrendMetric,
  type TrendPoint,
  type TrendSeries,
} from "./trend-chart";

export interface VisibilityViewProps {
  /** Selectable prompts derived from the live runs (declared prompt names). */
  prompts: ReadonlyArray<PromptOption>;
  /**
   * Live trend points keyed by provider. When non-empty for a given provider
   * we render that series. In demo mode this carries the mock generator's
   * output; otherwise it is the real `/api/visibility/trend` response.
   */
  liveByProvider: Readonly<Record<string, ReadonlyArray<TrendPoint>>>;
  /** Initial selection (driven by `?prompt=` search param when present). */
  initialPrompt: PromptOption;
  /** Initial timescale, mirrored from `?days=`. */
  initialDays: 1 | 7 | 30;
  /** True when the points above are mock data shown under `OGEO_DEMO=1`. */
  isDemo: boolean;
  /** True when there is no live data and the dashboard is not in demo mode. */
  isEmpty: boolean;
}

type Days = 1 | 7 | 30;

/**
 * Derive the winners/losers leaderboard from the live trend series. For each
 * provider with at least two buckets we compare the most-recent bucket's
 * avg-rank against the prior bucket (`now - was`); a negative delta means the
 * rank improved (lower is better), rendered as "up". Sorted by absolute move
 * so the biggest swings surface first. No mock-analytics involved.
 */
function deltaLeaderboardFromSeries(
  series: ReadonlyArray<TrendSeries>,
  promptName: string,
): DeltaLeaderboardRow[] {
  const rows: DeltaLeaderboardRow[] = [];
  for (const s of series) {
    const pts = s.points;
    if (pts.length < 2) continue;
    const now = pts[pts.length - 1]!.avg_rank;
    const was = pts[pts.length - 2]!.avg_rank;
    const delta = Number((now - was).toFixed(2));
    const dir = delta < -0.05 ? "up" : delta > 0.05 ? "down" : "flat";
    rows.push({ name: promptName, provider: s.provider, delta, was, now, dir });
  }
  return rows.sort((a, b) => Math.abs(b.delta) - Math.abs(a.delta));
}

export function VisibilityView({
  prompts,
  liveByProvider,
  initialPrompt,
  initialDays,
  isDemo,
  isEmpty,
}: VisibilityViewProps) {
  const [prompt, setPrompt] = useState<PromptOption>(initialPrompt);
  const [days, setDays] = useState<Days>(initialDays);
  const [metric, setMetric] = useState<TrendMetric>("rank");

  const providers = useMemo(
    () => Object.keys(liveByProvider),
    [liveByProvider],
  );

  const series: TrendSeries[] = useMemo(() => {
    return providers
      .map((p): TrendSeries | null => {
        const live = liveByProvider[p];
        if (live && live.length > 0) {
          return { provider: p, points: live.slice(-(days + 1)) };
        }
        return null;
      })
      .filter((s): s is TrendSeries => s !== null);
  }, [liveByProvider, providers, days]);

  const heatmapRows: HeatmapInputRow[] = useMemo(
    () =>
      series.map((s) => ({
        provider: s.provider,
        ranks: s.points
          .slice(-Math.min(30, s.points.length))
          .map((p) => p.avg_rank),
      })),
    [series],
  );

  const leaderboard = useMemo(
    () => deltaLeaderboardFromSeries(series, prompt.name),
    [series, prompt.name],
  );

  if (isEmpty) {
    return (
      <EmptyState
        title="No visibility data yet"
        message="Run a prompt against your providers to populate the trend, rank heatmap, and weekly movers."
      />
    );
  }

  return (
    <div className="flex flex-col gap-[12px]">
      <div className="flex flex-wrap items-center justify-between gap-[12px] border border-[color:var(--border)] bg-[color:var(--bg-elev)] p-[12px]">
        <div className="flex flex-wrap items-center gap-[12px]">
          <PromptPicker prompts={prompts} value={prompt} onChange={setPrompt} />
          <span className="h-[18px] w-px bg-[color:var(--border)]" />
          <SegControl<TrendMetric>
            value={metric}
            onChange={setMetric}
            options={[
              { value: "rank", label: "Avg rank" },
              { value: "presence", label: "Presence" },
            ]}
            ariaLabel="Metric"
          />
          <SegControl<string>
            value={String(days)}
            onChange={(v) => setDays(Number(v) as Days)}
            options={[
              { value: "1", label: "1d" },
              { value: "7", label: "7d" },
              { value: "30", label: "30d" },
            ]}
            ariaLabel="Window"
          />
        </div>
        <div className="flex items-center gap-[8px]">
          {isDemo && <DemoBadge />}
          <div className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
            prompt={prompt.id}
          </div>
        </div>
      </div>

      <Card
        eyebrow={`${prompt.name} · ${metric} · ${days}d`}
        title={prompt.text ?? prompt.name}
        action={
          <>
            {series.map((s) => (
              <Pill key={s.provider}>
                <ProviderDot provider={s.provider} />
                <span className="ml-[4px]">
                  {resolveProviderIdentity(s.provider).label}
                </span>
              </Pill>
            ))}
          </>
        }
      >
        <TrendChart series={series} metric={metric} anomalyBandTrailing={null} />
      </Card>

      <div className="grid gap-[12px] lg:grid-cols-[1.4fr_1fr]">
        <Card
          eyebrow={`rank heatmap · last ${heatmapRows[0]?.ranks.length ?? 0} days`}
          title="Where we showed up"
        >
          <RankHeatmap rows={heatmapRows} />
        </Card>
        <Card eyebrow="winners / losers" title="This window vs prior">
          {leaderboard.length > 0 ? (
            <DeltaLeaderboard rows={leaderboard} />
          ) : (
            <EmptyState
              title="Not enough trend history"
              message="The leaderboard needs at least two buckets per provider to compute a delta."
            />
          )}
        </Card>
      </div>
    </div>
  );
}
