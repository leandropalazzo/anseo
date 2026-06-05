"use client";

import { useMemo, useState } from "react";

import { DemoBadge } from "@/components/demo-badge";
import { EmptyState } from "@/components/ui/empty-state";
import { Card } from "@/components/ui/card";
import { Pill } from "@/components/ui/pill";
import { SegControl } from "@/components/ui/seg-control";
import { ProviderDot } from "@/components/ui/provider-dot";
import { Sparkline } from "@/components/charts/sparkline";
import { Icon } from "@/lib/icons";
import type {
  CitationGraph as CitationGraphData,
  CitationScore,
  CitationSummaryRow,
  CitationTrendPoint,
} from "@/lib/api";
import type { ProviderId } from "@/lib/provider-colors";

import { CitationGraph } from "./citation-graph";
import { CitationScoreCard } from "./citation-score-card";
import { DomainsByType } from "./domains-by-type";

type Tab = "table" | "graph" | "domains";

const PROVIDER_ROTATION: ReadonlyArray<ProviderId> = [
  "openai",
  "anthropic",
  "gemini",
  "perplexity",
];

export interface CitationsViewProps {
  /**
   * Citation rows from `/api/citations/summary`. In demo mode these are the
   * mock rows (projected into the live shape); otherwise the real response.
   */
  rows: ReadonlyArray<CitationSummaryRow>;
  graph: CitationGraphData;
  /** Composite footprint-health score. Null in demo mode (no live summary). */
  score: CitationScore | null;
  /** Per-domain hourly frequency series (7d) keyed by domain. */
  trend?: Record<string, CitationTrendPoint[]>;
  /** True when `rows`/`graph` are mock data shown under `OGEO_DEMO=1`. */
  isDemo: boolean;
  /** True when there is no live data and the dashboard is not in demo mode. */
  isEmpty: boolean;
}

export function CitationsView({ rows, graph, score, trend = {}, isDemo, isEmpty }: CitationsViewProps) {
  const [tab, setTab] = useState<Tab>("table");
  const [filter, setFilter] = useState("");

  // Project the live rows into the table shape. The summary endpoint carries
  // no historical signal, so we render a flat zero-trend sparkline — operators
  // can tell from the missing sign that no trend is available yet.
  const unified = useMemo(
    () =>
      rows
        .map((r, i) => ({
          domain: r.domain,
          frequency: r.frequency,
          source: r.source_type ?? "unknown",
          index: i,
        }))
        .filter((r) =>
          filter ? r.domain.toLowerCase().includes(filter.toLowerCase()) : true,
        ),
    [rows, filter],
  );

  if (isEmpty) {
    return (
      <EmptyState
        title="No citation data yet"
        message="Once your runs cite sources, the top domains, provider → domain network, and source-type breakdown appear here."
      />
    );
  }

  return (
    <div className="flex flex-col gap-[12px]">
      {score && <CitationScoreCard score={score} windowDays={30} />}
      <div className="flex flex-wrap items-center justify-between gap-[8px] border border-[color:var(--border)] bg-[color:var(--bg-elev)] p-[12px]">
        <div className="flex flex-wrap items-center gap-[8px]">
          <input
            placeholder="Filter by domain..."
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            className="min-w-[260px] appearance-none border border-[color:var(--border)] bg-[color:var(--bg-sunken)] px-[10px] py-[5px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-sm)] text-[color:var(--text)] outline-none"
          />
          <Pill>{unified.length} domains</Pill>
          <Pill>
            {unified.reduce((acc, x) => acc + x.frequency, 0)} citations
          </Pill>
          {isDemo && <DemoBadge />}
        </div>
        <SegControl<Tab>
          value={tab}
          onChange={setTab}
          options={[
            { value: "table", label: "Table" },
            { value: "graph", label: "Network" },
            { value: "domains", label: "By domain" },
          ]}
          ariaLabel="View"
        />
      </div>

      {tab === "table" && (
        <Card padding={false}>
          <table className="w-full border-collapse">
            <thead>
              <tr className="bg-[color:var(--bg-sunken)]">
                {["domain", "type", "frequency", "7d trend", "providers", ""].map((h) => (
                  <th
                    key={h}
                    className="border-b border-[color:var(--border)] px-[12px] py-[6px] text-left font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] font-medium uppercase tracking-[0.4px] text-[color:var(--text-faint)]"
                  >
                    {h}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {unified.map((c) => {
                const i = c.index;
                const provs: ProviderId[] = [
                  PROVIDER_ROTATION[i % 4]!,
                  PROVIDER_ROTATION[(i + 1) % 4]!,
                  ...(i % 3 === 0 ? [PROVIDER_ROTATION[(i + 2) % 4]!] : []),
                ];
                return (
                  <tr
                    key={`${c.domain}-${c.source}`}
                    className="border-b border-[color:var(--hairline)]"
                  >
                    <td className="px-[12px] py-[6px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-sm)] text-[color:var(--text)]">
                      {c.domain}
                    </td>
                    <td
                      data-testid={`citation-trend-${c.domain}`}
                      className="px-[12px] py-[6px]"
                    >
                      <Pill>{c.source}</Pill>
                    </td>
                    <td className="px-[12px] py-[6px] font-[family-name:var(--font-mono)] text-[color:var(--text)]">
                      {c.frequency}
                    </td>
                    <td className="px-[12px] py-[6px]">
                      {(() => {
                        const series = (trend[c.domain] ?? []).map(
                          (p) => p.frequency,
                        );
                        return series.length > 1 ? (
                          <Sparkline
                            points={series}
                            color="var(--accent)"
                            width={90}
                            ariaLabel={`${c.domain} citation trend`}
                            dataTestId={`citation-trend-sparkline-${c.domain}`}
                          />
                        ) : (
                          <span className="text-[color:var(--text-faint)]">—</span>
                        );
                      })()}
                    </td>
                    <td className="px-[12px] py-[6px]">
                      <div className="flex gap-[4px]">
                        {provs.map((p) => (
                          <ProviderDot key={p} provider={p} />
                        ))}
                      </div>
                    </td>
                    <td className="px-[12px] py-[6px] text-[color:var(--text-faint)]">
                      <Icon.ExternalLink size={11} strokeWidth={1.5} />
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </Card>
      )}

      {tab === "graph" && (
        <Card eyebrow="providers ↔ cited domains" title="Citation network">
          <CitationGraph graph={graph} />
        </Card>
      )}

      {tab === "domains" && <DomainsByType rows={rows} />}
    </div>
  );
}
