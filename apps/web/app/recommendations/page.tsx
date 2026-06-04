import Link from "next/link";

import { Card } from "@/components/ui/card";
import { Bar } from "@/components/charts/bar";
import {
  fetchRecommendationIntelligence,
  fetchRecommendations,
  SEVERITY_RANK,
  type KindAdoption,
  type Recommendation,
} from "@/lib/api";

import { GenerateButton } from "./_components/generate-button";
import { QuickActions } from "./_components/quick-actions";
import { NdpMarkerFor, PriorityLabel } from "./_components/priority-label";

export const dynamic = "force-dynamic";

function sortByPriority(items: Recommendation[]): Recommendation[] {
  return [...items].sort((a, b) => {
    const bySeverity = SEVERITY_RANK[b.severity] - SEVERITY_RANK[a.severity];
    if (bySeverity !== 0) return bySeverity;
    return b.generated_at.localeCompare(a.generated_at);
  });
}

export default async function RecommendationsPage() {
  let items: Recommendation[] = [];
  let error: string | null = null;
  try {
    const res = await fetchRecommendations({ limit: 50 });
    items = sortByPriority(res.items);
  } catch (e) {
    error = String(e);
  }

  let intelligence: KindAdoption[] = [];
  try {
    intelligence = (await fetchRecommendationIntelligence()).by_kind;
  } catch {
    intelligence = [];
  }

  return (
    <section
      data-testid="recommendations-page"
      className="flex flex-col gap-[12px]"
    >
      <header className="flex items-baseline justify-between">
        <div>
          <h1 className="m-0 text-[length:22px] font-normal tracking-[var(--display-tracking)] text-[color:var(--text)]">
            Recommendations
          </h1>
          <p className="m-0 mt-[2px] text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
            Active recommendations, highest priority first.
          </p>
        </div>
        <GenerateButton />
      </header>

      {intelligence.length > 0 && <IntelligencePanel rows={intelligence} />}

      {error ? (
        <Card>
          <div
            data-testid="recommendations-error"
            className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-sm)] text-[color:var(--warn)]"
          >
            {error}
          </div>
        </Card>
      ) : items.length === 0 ? (
        <Card>
          <div
            data-testid="recommendations-empty"
            className="text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]"
          >
            No active recommendations. Run{" "}
            <code className="font-[family-name:var(--font-mono)]">
              ogeo recommend generate
            </code>{" "}
            to produce them.
          </div>
        </Card>
      ) : (
        <ul
          data-testid="recommendations-list"
          className="m-0 flex list-none flex-col gap-[8px] p-0"
        >
          {items.map((rec) => (
            <li key={rec.id}>
              <Card>
                <div className="flex items-start justify-between gap-[12px]">
                  <Link
                    href={`/recommendations/${encodeURIComponent(rec.id)}`}
                    data-testid="rec-row"
                    data-rec-id={rec.id}
                    className="block min-w-0 flex-1"
                  >
                    <div className="min-w-0 flex flex-col gap-[4px]">
                      <div className="flex items-center gap-[8px]">
                        <PriorityLabel severity={rec.severity} />
                        <NdpMarkerFor rec={rec} />
                      </div>
                      <div className="text-[length:var(--font-size-sm)] text-[color:var(--text)]">
                        {rec.summary}
                      </div>
                      <div className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
                        {rec.kind} · {rec.state} · conf:{rec.confidence_band}
                      </div>
                    </div>
                  </Link>
                  <QuickActions id={rec.id} state={rec.state} />
                </div>
              </Card>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

/** "What works vs what doesn't" — per-kind adoption from the lifecycle:
 *  how many of each recommendation kind were acted on vs dismissed. */
function IntelligencePanel({ rows }: { rows: KindAdoption[] }) {
  const totalActed = rows.reduce((n, r) => n + r.acted, 0);
  const totalSurfaced = rows.reduce((n, r) => n + r.surfaced, 0);
  return (
    <Card
      eyebrow="what works"
      title="Adoption by recommendation kind"
      accent
      action={
        <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-muted)]">
          {totalSurfaced > 0 ? `${Math.round((totalActed / totalSurfaced) * 100)}% acted overall` : "—"}
        </span>
      }
    >
      <div className="flex flex-col gap-[8px]">
        {rows.map((r) => (
          <div key={r.kind} className="flex items-center gap-[10px]">
            <div className="w-[160px] shrink-0 truncate font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text)]">
              {r.kind}
            </div>
            <Bar
              value={r.acted}
              max={Math.max(1, r.surfaced)}
              color="var(--ok)"
              ariaLabel={`${r.kind} adoption`}
              className="flex-1"
            />
            <div className="w-[150px] shrink-0 text-right font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
              {r.acted} acted · {r.dismissed} dismissed
            </div>
          </div>
        ))}
      </div>
      <p className="mt-[10px] text-[length:var(--font-size-xs)] text-[color:var(--text-muted)]">
        Mark recommendations <strong>Acted</strong> (with evidence) on their detail page to feed
        this loop — it shows which kinds actually move your visibility.
      </p>
    </Card>
  );
}
