import { Card } from "@/components/ui/card";
import { DemoBadge } from "@/components/demo-badge";
import { EmptyState } from "@/components/ui/empty-state";
import { PageHeader } from "@/components/ui/page-header";
import {
  fetchBrands,
  fetchComparisons,
  type BrandItem,
  type ComparisonRow,
} from "@/lib/api";
import { demoOrEmpty } from "@/lib/data-source";

import { CompetitorTile } from "./_components/competitor-tile";
import { HeadToHead } from "./_components/head-to-head";
import { Movers, type MoverRow } from "./_components/movers";
import { ShareOfVoiceChart, type ShareRow } from "./_components/stacked-area";
import { WinLossTable, type WinLossRow } from "./_components/win-loss-table";
import type { ProviderId } from "@/lib/provider-colors";

const WINDOW: "1d" | "7d" | "30d" = "7d";
const PROVIDERS: ReadonlyArray<ProviderId> = [
  "openai",
  "anthropic",
  "gemini",
  "perplexity",
];

// ─── Derivations over the live comparison matrix ────────────────────────────
//
// The /comparisons contract is a snapshot: rows are per prompt×provider, each
// carrying a cell per subject with `mention_count` and an optional `ranking`.
// There is no daily/time dimension in the contract, so every chart here is a
// current-window snapshot (the legacy mock's 30-day series is demo-only).

/** Total mention_count per subject across the whole matrix. */
function mentionsBySubject(rows: ComparisonRow[]): Map<string, number> {
  const totals = new Map<string, number>();
  for (const row of rows) {
    for (const cell of row.cells) {
      totals.set(cell.subject, (totals.get(cell.subject) ?? 0) + cell.mention_count);
    }
  }
  return totals;
}

/** Share-of-voice snapshot: each subject's fraction of total mentions. */
function shareRows(rows: ComparisonRow[]): ShareRow[] {
  const totals = mentionsBySubject(rows);
  const grand = [...totals.values()].reduce((a, b) => a + b, 0);
  if (grand === 0) return [];
  return [...totals.entries()]
    .map(([name, count]) => ({ name, share: count / grand }))
    .sort((a, b) => b.share - a.share);
}

/**
 * Win/loss per competitor per provider. A competitor is "ahead of us" on a
 * provider when, restricted to that provider's rows, it out-ranks the primary
 * brand on average (lower `ranking` wins; when neither side has rankings we
 * fall back to higher total mention_count).
 */
function winLossRows(
  rows: ComparisonRow[],
  primary: string,
  competitors: string[],
): WinLossRow[] {
  // Per provider, accumulate ranking sum/count and mention totals per subject.
  type Acc = { rankSum: number; rankN: number; mentions: number };
  const byProvider = new Map<string, Map<string, Acc>>();
  for (const row of rows) {
    const subj = byProvider.get(row.provider) ?? new Map<string, Acc>();
    for (const cell of row.cells) {
      const a = subj.get(cell.subject) ?? { rankSum: 0, rankN: 0, mentions: 0 };
      a.mentions += cell.mention_count;
      if (typeof cell.ranking === "number") {
        a.rankSum += cell.ranking;
        a.rankN += 1;
      }
      subj.set(cell.subject, a);
    }
    byProvider.set(row.provider, subj);
  }

  const avgRank = (a?: Acc) => (a && a.rankN > 0 ? a.rankSum / a.rankN : null);

  return competitors.map((competitor) => {
    const ahead = {} as Record<ProviderId, boolean>;
    for (const p of PROVIDERS) {
      const subj = byProvider.get(p);
      const us = avgRank(subj?.get(primary));
      const them = avgRank(subj?.get(competitor));
      if (us !== null && them !== null) {
        ahead[p] = them < us; // lower ranking == better placement
      } else {
        const usM = subj?.get(primary)?.mentions ?? 0;
        const themM = subj?.get(competitor)?.mentions ?? 0;
        ahead[p] = themM > usM;
      }
    }
    // "Where they win": the provider(s) where this competitor is ahead of us.
    const wins = PROVIDERS.filter((p) => ahead[p]);
    return {
      competitor,
      ahead,
      whereTheyWin: wins.length ? wins.join(", ") : "—",
    };
  });
}

/**
 * Movers: competitors ordered by their share-of-voice snapshot. The contract
 * carries no historical delta, so `deltaPp` is left null and `avgRank` (from
 * /brands) is surfaced instead. Brands present in /brands but absent from the
 * comparison matrix are flagged as new entrants.
 */
function moverRows(
  share: ShareRow[],
  brands: BrandItem[],
  primary: string,
): MoverRow[] {
  const seen = new Set(share.map((s) => s.name.toLowerCase()));
  const rankByName = new Map(
    brands.map((b) => [b.name.toLowerCase(), b.avg_rank_7d ?? null]),
  );
  return share
    .filter((s) => s.name.toLowerCase() !== primary.toLowerCase())
    .slice(0, 6)
    .map((s) => ({
      name: s.name,
      share: s.share,
      avgRank: rankByName.get(s.name.toLowerCase()) ?? null,
      isNew: !seen.has(s.name.toLowerCase()) ? true : undefined,
    }))
    .concat(
      // Tracked brands not present in the comparison matrix at all.
      brands
        .filter(
          (b) =>
            !b.is_primary &&
            !seen.has(b.name.toLowerCase()) &&
            b.name.toLowerCase() !== primary.toLowerCase(),
        )
        .map((b) => ({
          name: b.name,
          share: 0,
          avgRank: b.avg_rank_7d ?? null,
          isNew: true,
        })),
    );
}

// ─── Demo fallbacks (only surface under IS_DEMO via demoOrEmpty) ────────────

function demoShare(): ShareRow[] {
  return [
    { name: "pinecone", share: 0.32 },
    { name: "qdrant", share: 0.21 },
    { name: "weaviate", share: 0.18 },
    { name: "milvus", share: 0.11 },
    { name: "chroma", share: 0.1 },
    { name: "lancedb", share: 0.08 },
  ];
}

export default async function CompetitorsPage() {
  // Roster first: derive the primary brand + competitor set from /brands.
  let brands: BrandItem[] = [];
  let comparisonRows: ComparisonRow[] = [];
  let competitorsFromCompare: string[] = [];

  try {
    const brandsResp = await fetchBrands();
    brands = brandsResp.items ?? [];
  } catch {
    brands = [];
  }

  const primaryItem = brands.find((b) => b.is_primary) ?? brands[0];
  const primary = primaryItem?.name ?? "";
  const competitorNames = brands
    .filter((b) => !b.is_primary)
    .map((b) => b.name);

  // `/v1/comparisons` accepts 2..=6 brands (MCP compare_brands contract). Send
  // the primary plus its top-5 competitors by 7d mention count; sending the
  // full roster would 400 and blank the whole page.
  const topCompetitors = brands
    .filter((b) => !b.is_primary)
    .sort((a, b) => b.mention_count_7d - a.mention_count_7d)
    .slice(0, 5)
    .map((b) => b.name);

  if (primary) {
    try {
      const cmp = await fetchComparisons(
        [primary, ...topCompetitors],
        WINDOW,
      );
      comparisonRows = cmp.rows ?? [];
      competitorsFromCompare = cmp.competitors ?? [];
    } catch {
      comparisonRows = [];
    }
  }

  const liveShare = shareRows(comparisonRows);
  const competitors =
    competitorsFromCompare.length > 0 ? competitorsFromCompare : competitorNames;

  // Resolve against the demo-data contract: live snapshot, else EmptyState,
  // else (demo mode only) mock with a visible DemoBadge.
  const resolved = demoOrEmpty<ShareRow>(liveShare, demoShare);
  const share = resolved.data;
  const winLossLive =
    primary && comparisonRows.length
      ? winLossRows(comparisonRows, primary, competitors)
      : [];
  const moversLive = moverRows(share, brands, primary || share[0]?.name || "");

  const top4 = share.slice(0, 4);
  const headToHeadPair = competitors.slice(0, 2);

  return (
    <section data-testid="competitors-page" className="space-y-[12px]">
      <PageHeader
        title="Competitors"
        description="Share-of-voice, head-to-head, movers, and weekly win/loss roll-up."
        actions={resolved.isDemo ? <DemoBadge /> : undefined}
      />

      {resolved.isEmpty ? (
        <Card eyebrow="competitors" title="No comparison data yet">
          <EmptyState
            title="No competitor data yet"
            message="Run a prompt against your tracked brands to populate share-of-voice, head-to-head, and win/loss."
          />
        </Card>
      ) : (
        <>
          <Card
            eyebrow={`share of voice · ${(primary || share[0]?.name || "brand").toLowerCase()} vs ${Math.max(0, share.length - 1)} competitors`}
            title={`Share of voice · last ${WINDOW}`}
          >
            <ShareOfVoiceChart rows={share} primary={primary} />
          </Card>

          <div className="grid gap-[12px] lg:grid-cols-2">
            <Card
              eyebrow="head-to-head"
              title={
                headToHeadPair.length === 2
                  ? `${headToHeadPair[0]} vs ${headToHeadPair[1]}`
                  : "Head-to-head"
              }
            >
              {headToHeadPair.length === 2 ? (
                <HeadToHead
                  rows={share}
                  a={headToHeadPair[0]!}
                  b={headToHeadPair[1]!}
                />
              ) : (
                <EmptyState
                  title="Need two competitors"
                  message="Track at least two competitor brands to see a head-to-head."
                />
              )}
            </Card>
            <Card eyebrow="competitors by share" title="Movers">
              <Movers rows={moversLive} />
            </Card>
          </div>

          <Card eyebrow="top 4 by share · last day" title="Competitor tiles">
            <div className="grid gap-[12px] sm:grid-cols-2 lg:grid-cols-4">
              {top4.map((c) => (
                <CompetitorTile
                  key={c.name}
                  name={c.name}
                  share={c.share}
                  accent={c.name.toLowerCase() === primary.toLowerCase()}
                />
              ))}
            </div>
          </Card>

          <Card eyebrow="weekly digest" title="Where competitors win">
            {winLossLive.length ? (
              <WinLossTable rows={winLossLive} />
            ) : (
              <EmptyState
                title="No ranked comparisons yet"
                message="Win/loss is derived from provider rankings; run more prompts to populate it."
              />
            )}
          </Card>
        </>
      )}
    </section>
  );
}
