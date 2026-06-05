import { Card } from "@/components/ui/card";
import { PageHeader } from "@/components/ui/page-header";
import { fetchAuditRuns, fetchBrandConfig, type AuditRunItem } from "@/lib/api";

import { AuditRunner } from "./_components/audit-runner";

export default async function AuditPage() {
  let brandName: string | undefined;
  let siteUrl: string | undefined;
  try {
    const brand = await fetchBrandConfig();
    brandName = brand.name;
    siteUrl = brand.site_url;
  } catch {
    brandName = undefined;
    siteUrl = undefined;
  }

  let history: AuditRunItem[] = [];
  try {
    history = (await fetchAuditRuns(20)).items;
  } catch {
    history = [];
  }

  return (
    <section data-testid="audit-page" className="space-y-[12px]">
      <PageHeader
        title="Site Audit"
        description={<>Crawl{" "}{brandName ? <span className="text-[color:var(--text)]">{brandName}</span> : "your"}{" "}owned pages and score citation-readiness against open, in-tree heuristics.</>}
      />
      <AuditRunner initialTarget={siteUrl} brandName={brandName} />

      {history.length > 0 && (
        <Card eyebrow="history" title="Past audits">
          <table className="w-full border-collapse text-[length:var(--font-size-sm)]">
            <thead>
              <tr className="border-b border-[color:var(--hairline)] text-left text-[color:var(--text-faint)]">
                <th className="px-[8px] py-[6px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] font-normal uppercase">
                  When
                </th>
                <th className="px-[8px] py-[6px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] font-normal uppercase">
                  Target
                </th>
                <th className="px-[8px] py-[6px] text-right font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] font-normal uppercase">
                  Pages
                </th>
                <th className="px-[8px] py-[6px] text-right font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] font-normal uppercase">
                  Score
                </th>
              </tr>
            </thead>
            <tbody>
              {history.map((h) => (
                <tr key={h.id} className="border-b border-[color:var(--hairline)]">
                  <td className="px-[8px] py-[6px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-muted)]">
                    {new Date(h.created_at).toLocaleString()}
                  </td>
                  <td className="px-[8px] py-[6px] font-[family-name:var(--font-mono)] text-[color:var(--text)]">
                    {h.target}
                  </td>
                  <td className="px-[8px] py-[6px] text-right tabular-nums text-[color:var(--text-muted)]">
                    {h.pages_crawled}
                  </td>
                  <td className="px-[8px] py-[6px] text-right tabular-nums text-[color:var(--text)]">
                    {h.overall_score}/100
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </Card>
      )}
    </section>
  );
}
