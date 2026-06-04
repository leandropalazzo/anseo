import {
  fetchCitationGraph,
  fetchCitationSummary,
  fetchCitationTrend,
  type CitationGraph as CitationGraphData,
  type CitationScore,
  type CitationSummaryRow,
  type CitationTrendPoint,
} from "@/lib/api";
import { demoOrEmpty, IS_DEMO } from "@/lib/data-source";

import { CitationsView } from "./_components/citations-view";

const EMPTY_GRAPH: CitationGraphData = { nodes: [], edges: [] };

/** Demo-only citation rows + network graph. Imported lazily so the mock
 *  modules never reach the bundle/render path unless `OGEO_DEMO=1`. */
async function demoCitations(): Promise<{
  rows: CitationSummaryRow[];
  graph: CitationGraphData;
}> {
  const { CITATIONS } = await import("@/lib/mock");
  const { mockCitationGraph } = await import("@/lib/mock-analytics");
  const rows: CitationSummaryRow[] = CITATIONS.map((c) => ({
    domain: c.domain,
    frequency: c.frequency,
    source_type: c.source,
  }));
  return { rows, graph: mockCitationGraph() };
}

export default async function CitationsPage() {
  let liveRows: CitationSummaryRow[] = [];
  let score: CitationScore | null = null;
  try {
    const r = await fetchCitationSummary(50);
    liveRows = r.domains;
    score = r.citation_score;
  } catch {
    liveRows = [];
  }

  let liveGraph: CitationGraphData = EMPTY_GRAPH;
  try {
    liveGraph = await fetchCitationGraph(30);
  } catch {
    liveGraph = EMPTY_GRAPH;
  }

  let trend: Record<string, CitationTrendPoint[]> = {};
  try {
    trend = await fetchCitationTrend(7 * 24, 50);
  } catch {
    trend = {};
  }

  // Demo-data contract: live present → render it; empty + demo → mock (with a
  // visible <DemoBadge/>); empty + not demo → the view renders an EmptyState.
  // Both surfaces share one demo decision so the badge and graph stay in sync.
  const rowsResult = demoOrEmpty(liveRows, () => [] as CitationSummaryRow[]);
  let rows = rowsResult.data;
  let graph = liveGraph;
  let isDemo = false;
  let isEmpty = rowsResult.isEmpty;

  if (liveRows.length === 0 && liveGraph.nodes.length === 0 && IS_DEMO) {
    const demo = await demoCitations();
    rows = demo.rows;
    graph = demo.graph;
    isDemo = true;
    isEmpty = false;
  }

  return (
    <section data-testid="citations-page" className="space-y-[12px]">
      <header>
        <h1 className="m-0 text-[length:22px] font-normal tracking-[var(--display-tracking)] text-[color:var(--text)]">
          Citations
        </h1>
        <p className="m-0 mt-[2px] text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
          Top domains, the provider → domain citation network, and a
          source-type breakdown.
        </p>
      </header>
      <CitationsView
        rows={rows}
        graph={graph}
        score={score}
        trend={trend}
        isDemo={isDemo}
        isEmpty={isEmpty}
      />
    </section>
  );
}
