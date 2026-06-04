import Link from "next/link";

import {
  SEVERITY_RANK,
  type Recommendation,
} from "@/lib/api";

import {
  NdpMarkerFor,
  PriorityLabel,
} from "../recommendations/_components/priority-label";

// Story 19.9 / UX-DR126 — the Overview "Top Recommendations" tile renders the
// top 3 active recs by priority using the SAME shared PriorityLabel + NdpMarker
// components as the /recommendations list, so a given envelope renders
// identically across surfaces. The cross-surface byte-identity is asserted in
// tests/component/cross-surface.test.tsx and the Rust contract test.

export function TopRecommendations({ items }: { items: Recommendation[] }) {
  const top = [...items]
    .sort((a, b) => {
      const s = SEVERITY_RANK[b.severity] - SEVERITY_RANK[a.severity];
      return s !== 0 ? s : b.generated_at.localeCompare(a.generated_at);
    })
    .slice(0, 3);

  if (top.length === 0) {
    return (
      <div
        data-testid="top-recs-empty"
        className="text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]"
      >
        No active recommendations.
      </div>
    );
  }

  return (
    <ul
      data-testid="top-recs-list"
      className="m-0 flex list-none flex-col gap-[8px] p-0"
    >
      {top.map((rec) => (
        <li key={rec.id}>
          <Link
            href={`/recommendations/${encodeURIComponent(rec.id)}`}
            data-testid="top-rec-row"
            data-rec-id={rec.id}
            className="flex flex-col gap-[3px] border border-[color:var(--border)] px-[10px] py-[7px] hover:border-[color:var(--accent)]"
          >
            <div className="flex items-center gap-[8px]">
              <PriorityLabel severity={rec.severity} />
              <NdpMarkerFor rec={rec} />
            </div>
            <span className="text-[length:var(--font-size-sm)] text-[color:var(--text)]">
              {rec.summary}
            </span>
          </Link>
        </li>
      ))}
    </ul>
  );
}
