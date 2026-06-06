// Story 47.4 — Operator Site Analytics dashboard.
//
// Operator-facing read view over the public-site event rollups (Epic 47). Four
// panels: Site Overview (sessions sparkline + top pages + top referrers),
// Contribute Funnel (step counts + drop-off), Verify Funnel (start/complete/
// fail by method), and Badge Embeds (daily serves, last 30 d).
//
// Server component: reads `searchParams.period` (7d|30d) and fetches both
// endpoints server-side so the operator API key never reaches the browser. The
// 7d/30d toggle navigates `?period=...` to re-render. Every panel degrades to an
// empty state when the rollup table is empty (AC-5) — and when ALL panels are
// empty the page shows a single first-run zero-state.

import { Card } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/empty-state";
import { PageHeader } from "@/components/ui/page-header";
import {
  fetchFunnels,
  fetchSiteOverview,
  isAnalyticsEmpty,
  normalizePeriod,
} from "@/lib/api";

import { FunnelChart } from "./_components/funnel-chart";
import { PeriodToggle } from "./_components/period-toggle";
import { RankedList } from "./_components/ranked-list";
import { SparkBars } from "./_components/spark-bars";
import { VerifyFunnel } from "./_components/verify-funnel";

export const dynamic = "force-dynamic";

interface SearchParams {
  period?: string;
}

export default async function AnalyticsPage({
  searchParams,
}: {
  searchParams: Promise<SearchParams>;
}) {
  const sp = await searchParams;
  const period = normalizePeriod(sp.period);

  const [overview, funnels] = await Promise.all([
    fetchSiteOverview(period),
    fetchFunnels(period),
  ]);

  const empty = isAnalyticsEmpty(overview, funnels);

  return (
    <section className="flex flex-col gap-[14px]" data-testid="analytics-page">
      <PageHeader
        title="Site Analytics"
        description="Privacy-safe traffic & funnel health for the public site, from aggregate rollups."
        actions={<PeriodToggle value={period} />}
      />

      {empty ? (
        <EmptyState
          title="No data yet"
          hint="Events will appear once the public site is instrumented and the nightly rollup runs."
        />
      ) : (
        <>
          {/* ── Site Overview ─────────────────────────────────────────── */}
          <Card eyebrow="Site Overview" title={`Unique sessions · last ${overview.period_days}d`}>
            {overview.sessions_per_day.length === 0 ? (
              <EmptyState
                title="No sessions in this window"
                hint="page_view events drive this sparkline"
              />
            ) : (
              <SparkBars
                data={overview.sessions_per_day}
                colorIndex={0}
                label="Unique sessions per day"
              />
            )}
          </Card>

          <div className="grid grid-cols-1 gap-[14px] md:grid-cols-2">
            <Card eyebrow="Site Overview" title="Top pages">
              <RankedList
                rows={overview.top_pages.map((p) => ({
                  label: p.path,
                  value: p.views,
                }))}
                unitLabel="views"
                emptyTitle="No page views yet"
                emptyHint="top pages appear once page_view rollups exist"
              />
            </Card>

            <Card eyebrow="Site Overview" title="Top referrers">
              <RankedList
                rows={overview.top_referrers.map((r) => ({
                  label: r.domain,
                  value: r.visits,
                }))}
                unitLabel="visits"
                emptyTitle="No referrers yet"
                emptyHint="referrer domains appear once external traffic is seen"
              />
            </Card>
          </div>

          {/* ── Contribute Funnel ─────────────────────────────────────── */}
          <Card
            eyebrow="Contribute Funnel"
            title={`Step conversion · last ${funnels.period_days}d`}
          >
            {funnels.contribute.every((s) => s.count === 0) ? (
              <EmptyState
                title="No contribute activity"
                hint="contribute_start / _step / _complete events populate this funnel"
              />
            ) : (
              <FunnelChart steps={funnels.contribute} />
            )}
          </Card>

          {/* ── Verify Funnel ─────────────────────────────────────────── */}
          <Card
            eyebrow="Verify Funnel"
            title={`Verification by method · last ${funnels.period_days}d`}
          >
            <VerifyFunnel methods={funnels.verify} />
          </Card>

          {/* ── Badge Embeds ──────────────────────────────────────────── */}
          <Card eyebrow="Badge Embeds" title="Daily serves · last 30d">
            {funnels.badge_embeds_per_day.length === 0 ? (
              <EmptyState
                title="No badge embeds yet"
                hint="badge_embed_view events (server-side) populate this chart"
              />
            ) : (
              <SparkBars
                data={funnels.badge_embeds_per_day}
                colorIndex={2}
                label="Badge embed serves per day"
              />
            )}
          </Card>
        </>
      )}
    </section>
  );
}
