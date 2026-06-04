import { Card } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/empty-state";
import {
  fetchHallucinationSummary,
  type ClaimVerdict,
  type HallucinationSummary,
} from "@/lib/api";

const STATUS_COLOR: Record<string, string> = {
  accurate: "var(--ok)",
  inaccurate: "var(--danger)",
  unverifiable: "var(--text-faint)",
  premium_disabled: "var(--text-faint)",
};

export default async function HallucinationPage() {
  let summary: HallucinationSummary | null = null;
  try {
    summary = await fetchHallucinationSummary(30);
  } catch {
    summary = null;
  }

  const isPremium = summary?.entitlement === "premium_enabled";
  const totals = summary?.totals;
  const inaccurate = (summary?.recent ?? []).filter((c) => c.status === "inaccurate");

  return (
    <section data-testid="hallucination-page" className="space-y-[12px]">
      <header>
        <h1 className="m-0 text-[length:22px] font-normal tracking-[var(--display-tracking)] text-[color:var(--text)]">
          Brand Accuracy
        </h1>
        <p className="m-0 mt-[2px] text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
          Factual claims AI engines make about your brand, checked against ground-truth facts.
        </p>
      </header>

      {!summary ? (
        <EmptyState
          title="No claims collected yet"
          hint="Run prompts so the extractor can collect factual claims."
        />
      ) : (
        <>
          {!isPremium && (
            <Card eyebrow="open-core" title="Hallucination judgment is a premium capability" accent>
              <p className="text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
                The OSS build collects {totals?.total ?? 0} claim
                {totals?.total === 1 ? "" : "s"} and {summary.ground_truth_facts} ground-truth fact
                {summary.ground_truth_facts === 1 ? "" : "s"}, but accuracy verdicts require a premium
                entitlement. Claims below show as <em>premium-disabled</em> until judgment is enabled.
              </p>
            </Card>
          )}

          <div className="grid grid-cols-2 gap-[12px] md:grid-cols-4">
            <Tile label="Accurate" value={totals?.accurate ?? 0} color="var(--ok)" />
            <Tile label="Inaccurate" value={totals?.inaccurate ?? 0} color="var(--danger)" />
            <Tile label="Unverifiable" value={totals?.unverifiable ?? 0} color="var(--text-faint)" />
            <Tile label="Facts" value={summary.ground_truth_facts} color="var(--text-muted)" />
          </div>

          {isPremium && (
            <Card eyebrow="alerts" title="Inaccurate claims">
              {inaccurate.length === 0 ? (
                <p className="text-[length:var(--font-size-sm)] text-[color:var(--ok)]">
                  No inaccurate claims detected in the window.
                </p>
              ) : (
                <ul className="space-y-[8px]">
                  {inaccurate.map((c, i) => (
                    <ClaimRow key={`${c.prompt_run_id}-${i}`} claim={c} />
                  ))}
                </ul>
              )}
            </Card>
          )}

          <Card eyebrow="claims" title="Recent claims">
            {summary.recent.length === 0 ? (
              <EmptyState title="No claims yet" hint="CLI parity: claim collection runs in the extractor." />
            ) : (
              <ul className="space-y-[8px]">
                {summary.recent.map((c, i) => (
                  <ClaimRow key={`all-${c.prompt_run_id}-${i}`} claim={c} />
                ))}
              </ul>
            )}
          </Card>
        </>
      )}
    </section>
  );
}

function Tile({ label, value, color }: { label: string; value: number; color: string }) {
  return (
    <Card padding>
      <div className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] uppercase text-[color:var(--text-faint)]">
        {label}
      </div>
      <div className="mt-[4px] text-[length:24px]" style={{ color }}>
        {value}
      </div>
    </Card>
  );
}

function ClaimRow({ claim }: { claim: ClaimVerdict }) {
  return (
    <li className="flex items-start gap-[8px] text-[length:var(--font-size-sm)]">
      <span
        className="mt-[5px] inline-block h-[6px] w-[6px] shrink-0"
        style={{ background: STATUS_COLOR[claim.status] ?? "var(--text-faint)" }}
        aria-hidden
      />
      <div className="min-w-0">
        <div className="text-[color:var(--text)]">{claim.claim_text}</div>
        <div className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
          {claim.entity} · {claim.status}
          {claim.matched_fact_key ? ` · ${claim.matched_fact_key}` : ""}
        </div>
      </div>
    </li>
  );
}
