"use client";

import { useMemo, useState } from "react";

import { EmptyState } from "@/components/ui/empty-state";
import { Card } from "@/components/ui/card";
import { Pill } from "@/components/ui/pill";
import { SegControl } from "@/components/ui/seg-control";
import type { VisibilityMatrixCell, VisibilityPoint } from "@/lib/api";

import { RankHeatmap, type HeatmapInputRow } from "./rank-heatmap";
import { VisibilityMatrix, type MatrixMetric } from "./visibility-matrix";

export interface OverallVisibilityViewProps {
  brand: string;
  windowDays: number;
  matrix: ReadonlyArray<VisibilityMatrixCell>;
  trend: ReadonlyArray<VisibilityPoint>;
}

/** Build a date-aligned per-provider rank grid from the all-prompts trend so
 *  every provider row shares the same day columns (null = no run that day). */
function trendToHeatmapRows(
  trend: ReadonlyArray<VisibilityPoint>,
): HeatmapInputRow[] {
  const dates = Array.from(new Set(trend.map((p) => p.bucket_start))).sort();
  const providers = Array.from(new Set(trend.map((p) => p.provider))).sort();
  const byKey = new Map(trend.map((p) => [`${p.bucket_start} ${p.provider}`, p]));
  return providers.map((provider) => ({
    provider,
    ranks: dates.map((d) => {
      const pt = byKey.get(`${d} ${provider}`);
      return pt ? pt.avg_rank : null;
    }),
  }));
}

export function OverallVisibilityView({
  brand,
  windowDays,
  matrix,
  trend,
}: OverallVisibilityViewProps) {
  const [metric, setMetric] = useState<MatrixMetric>("presence");
  const heatmapRows = useMemo(() => trendToHeatmapRows(trend), [trend]);
  const dayCount = heatmapRows[0]?.ranks.length ?? 0;

  if (matrix.length === 0) {
    return (
      <EmptyState
        title="No visibility data yet"
        message="Run your prompts against your providers to populate the overall matrix and trend."
      />
    );
  }

  return (
    <div className="flex flex-col gap-[12px]">
      <div className="flex flex-wrap items-center justify-between gap-[12px] border border-[color:var(--border)] bg-[color:var(--bg-elev)] p-[12px]">
        <div className="flex flex-wrap items-center gap-[12px]">
          <SegControl<MatrixMetric>
            value={metric}
            onChange={setMetric}
            options={[
              { value: "presence", label: "Presence" },
              { value: "rank", label: "Avg rank" },
            ]}
            ariaLabel="Matrix metric"
          />
          {brand && <Pill>brand: {brand}</Pill>}
        </div>
        <div className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
          last {windowDays}d · all prompts
        </div>
      </div>

      <Card
        eyebrow={`prompt × provider · ${metric}`}
        title="Where we show up, across every prompt"
      >
        <VisibilityMatrix cells={matrix} metric={metric} />
      </Card>

      <Card
        eyebrow={`all prompts · avg rank · last ${dayCount} days`}
        title="Aggregate visibility trend"
      >
        {heatmapRows.length > 0 ? (
          <RankHeatmap rows={heatmapRows} />
        ) : (
          <EmptyState
            title="No trend yet"
            message="Once runs accumulate across days, the aggregate rank trend appears here."
          />
        )}
      </Card>
    </div>
  );
}
