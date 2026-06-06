import Link from "next/link";
import { notFound } from "next/navigation";
import { ArrowLeft } from "lucide-react";

import { Card } from "@/components/ui/card";
import { Pill } from "@/components/ui/pill";
import {
  fetchRecommendationDetail,
  isNonDeterministic,
  type Recommendation,
} from "@/lib/api";

import { EvidenceChips } from "../_components/evidence-chip";
import { NdpMarkerFor, PriorityLabel } from "../_components/priority-label";
import { LifecycleActions } from "./_components/lifecycle-actions";

export const dynamic = "force-dynamic";

export default async function RecommendationDetailPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = await params;

  let rec: Recommendation;
  try {
    rec = await fetchRecommendationDetail(id);
  } catch (e) {
    if (e instanceof Error && e.message.includes("404")) notFound();
    return (
      <section data-testid="rec-detail-error" className="space-y-2">
        <Link
          href="/recommendations"
          className="inline-flex items-center gap-[5px] text-[length:var(--font-size-sm)] text-[color:var(--text-muted)] hover:text-[color:var(--text)]"
        >
          <ArrowLeft size={11} strokeWidth={1.5} /> Recommendations
        </Link>
        <p className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-sm)] text-[color:var(--warn)]">
          {String(e)}
        </p>
      </section>
    );
  }

  const ndp = isNonDeterministic(rec);

  return (
    <section
      data-testid="rec-detail-page"
      className="flex flex-col gap-[12px]"
    >
      <div className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
        <Link
          href="/recommendations"
          className="inline-flex items-center gap-[5px] border border-[color:var(--border)] px-[8px] py-[4px] text-[color:var(--text-muted)] hover:text-[color:var(--text)]"
        >
          <ArrowLeft size={11} strokeWidth={1.5} /> Recommendations
        </Link>
      </div>

      <Card>
        <div className="flex flex-col gap-[10px]">
          <div className="flex items-center gap-[8px]">
            <PriorityLabel severity={rec.severity} />
            <NdpMarkerFor rec={rec} />
            <Pill mono>{rec.state}</Pill>
          </div>
          <h1 className="m-0 text-[length:var(--font-size-xl)] font-normal text-[color:var(--text)]">
            {rec.summary}
          </h1>
          <div className="flex flex-wrap gap-[6px]">
            <Pill mono>kind: {rec.kind}</Pill>
            <Pill mono>conf: {rec.confidence_band}</Pill>
            <Pill mono>repro: {rec.reproducibility.class}</Pill>
            <Pill mono>{rec.engine_version}</Pill>
          </div>
          {/* UX-DR109 — non-deterministic recs must not assert hard outcomes;
              we replace any quantitative claim with a variance disclaimer. */}
          {ndp && (
            <div
              data-testid="rec-ndp-disclaimer"
              className="border border-[color:var(--warn)] bg-[color:color-mix(in_oklch,var(--warn)_8%,transparent)] px-[10px] py-[6px] text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]"
            >
              This recommendation came off a non-deterministic pipeline.
              Outcomes are directional, not guaranteed — re-running may produce
              different results.
            </div>
          )}
        </div>
      </Card>

      <Card>
        <h2 className="m-0 mb-[8px] text-[length:var(--font-size-sm)] font-medium text-[color:var(--text)]">
          Evidence &amp; traceability
        </h2>
        <EvidenceChips traceability={rec.traceability} />
      </Card>

      <Card>
        <h2 className="m-0 mb-[8px] text-[length:var(--font-size-sm)] font-medium text-[color:var(--text)]">
          Actions
        </h2>
        <LifecycleActions rec={rec} />
      </Card>
    </section>
  );
}
