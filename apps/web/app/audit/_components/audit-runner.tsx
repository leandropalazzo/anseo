"use client";

import { useRouter } from "next/navigation";
import { useState } from "react";

import { Card } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/empty-state";
import type { AuditFinding, AuditReport, PageAudit } from "@/lib/api";

const SEVERITY_COLOR: Record<string, string> = {
  high: "var(--danger)",
  medium: "var(--warn)",
  low: "var(--text-faint)",
};

export function AuditRunner({
  initialTarget,
  brandName,
}: {
  initialTarget?: string;
  brandName?: string;
}) {
  const router = useRouter();
  const [target, setTarget] = useState(initialTarget ?? "");
  const [report, setReport] = useState<AuditReport | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function run(e: React.FormEvent) {
    e.preventDefault();
    if (!target.trim()) return;
    setLoading(true);
    setError(null);
    setReport(null);
    try {
      const r = await fetch("/api/audit", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ target: target.trim() }),
      });
      if (!r.ok) throw new Error(`audit failed (${r.status})`);
      setReport((await r.json()) as AuditReport);
      router.refresh(); // pull the just-persisted run into the history list
    } catch (err) {
      setError(err instanceof Error ? err.message : "audit failed");
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="space-y-[12px]">
      <Card
        eyebrow={brandName ? `target · ${brandName}` : "target"}
        title="Run a citation-readiness audit"
      >
        {brandName && !initialTarget && (
          <p className="mb-[8px] text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
            Tip: set {brandName}&rsquo;s website in{" "}
            <a
              href="/settings"
              className="text-[color:var(--accent)] underline-offset-2 hover:underline"
            >
              Settings → Brand
            </a>{" "}
            to prefill this target.
          </p>
        )}
        <form onSubmit={run} className="flex items-center gap-[8px]">
          <input
            value={target}
            onChange={(e) => setTarget(e.target.value)}
            placeholder="https://example.com or sitemap URL"
            className="flex-1 border border-[color:var(--border)] bg-[color:var(--bg-sunken)] px-[10px] py-[7px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-sm)] text-[color:var(--text)] outline-none focus:border-[color:var(--border-strong)]"
            data-testid="audit-target-input"
          />
          <button
            type="submit"
            disabled={loading || !target.trim()}
            className="border border-[color:var(--border-strong)] bg-[color:var(--accent)] px-[14px] py-[7px] text-[length:var(--font-size-sm)] text-[color:var(--accent-ink)] disabled:opacity-50"
            data-testid="audit-run-button"
          >
            {loading ? "Auditing…" : "Audit"}
          </button>
        </form>
        <p className="mt-[8px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
          CLI parity: ogeo audit &lt;url&gt; · MCP: audit tool
        </p>
        {error && (
          <p className="mt-[8px] text-[length:var(--font-size-sm)] text-[color:var(--danger)]">
            {error}
          </p>
        )}
      </Card>

      {!report ? (
        !loading && (
          <EmptyState
            title="No audit run yet"
            hint="Enter a URL above to score Identity, Extractability, and Corroboration."
          />
        )
      ) : (
        <>
          <Card eyebrow="overall" title={report.target} accent>
            <div className="flex items-baseline justify-between">
              <div className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-muted)]">
                {report.pages.length} page{report.pages.length === 1 ? "" : "s"} crawled
              </div>
              <div className="text-[length:var(--font-size-3xl)] text-[color:var(--text)]">
                {report.overall_score}
                <span className="text-[length:var(--font-size-sm)] text-[color:var(--text-faint)]">
                  /100
                </span>
              </div>
            </div>
            {report.gate && (
              <div className="mt-[8px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)]">
                gate:{" "}
                <span
                  style={{ color: report.gate.passed ? "var(--ok)" : "var(--danger)" }}
                >
                  {report.gate.passed ? "pass" : "fail"}
                </span>{" "}
                ({report.gate.fail_on.join(", ")})
              </div>
            )}
          </Card>

          {report.pages.map((page) => (
            <PageCard key={page.url} page={page} />
          ))}
        </>
      )}
    </div>
  );
}

function PageCard({ page }: { page: PageAudit }) {
  const violations = page.findings.filter((f) => f.status !== "pass");
  return (
    <Card
      eyebrow={page.title ?? "page"}
      title={page.url}
      action={
        <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-sm)] text-[color:var(--text)]">
          {page.score}/100
        </span>
      }
    >
      {violations.length === 0 ? (
        <p className="text-[length:var(--font-size-sm)] text-[color:var(--ok)]">
          No violations — fully citation-ready.
        </p>
      ) : (
        <ul className="space-y-[6px]">
          {violations.map((f) => (
            <FindingRow key={`${page.url}-${f.rule_id}`} finding={f} />
          ))}
        </ul>
      )}
    </Card>
  );
}

function FindingRow({ finding }: { finding: AuditFinding }) {
  return (
    <li className="flex items-start gap-[8px] text-[length:var(--font-size-sm)]">
      <span
        className="mt-[5px] inline-block h-[6px] w-[6px] shrink-0"
        style={{ background: SEVERITY_COLOR[finding.severity] ?? "var(--text-faint)" }}
        aria-hidden
      />
      <div>
        <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
          [{finding.category}/{finding.severity}] {finding.rule_id}
        </span>
        <div className="text-[color:var(--text)]">{finding.message}</div>
      </div>
    </li>
  );
}
