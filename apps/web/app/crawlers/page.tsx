import Link from "next/link";

import { Card } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/empty-state";
import {
  fetchBrands,
  fetchCrawlerMetrics,
  fetchCrawlReferRatio,
  type CrawlerMetrics,
  type CrawlReferReport,
} from "@/lib/api";

import { ConnectSource } from "./_components/connect-source";

interface SearchParams {
  days?: string;
}

function pickDays(raw: string | undefined): 7 | 30 | 90 {
  const n = Number(raw);
  if (n === 7 || n === 30 || n === 90) return n;
  return 30;
}

const WINDOWS: ReadonlyArray<7 | 30 | 90> = [7, 30, 90];

export default async function CrawlersPage({
  searchParams,
}: {
  searchParams: Promise<SearchParams>;
}) {
  const sp = await searchParams;
  const days = pickDays(sp.days);

  let metrics: CrawlerMetrics | null = null;
  let ratio: CrawlReferReport | null = null;
  try {
    metrics = await fetchCrawlerMetrics(days);
  } catch {
    metrics = null;
  }
  try {
    ratio = await fetchCrawlReferRatio(days);
  } catch {
    ratio = null;
  }

  let primaryBrand: string | undefined;
  try {
    const brands = await fetchBrands();
    primaryBrand = (brands.items.find((b) => b.is_primary) ?? brands.items[0])?.name;
  } catch {
    primaryBrand = undefined;
  }

  const hasBots = (metrics?.bots.length ?? 0) > 0;

  return (
    <section data-testid="crawlers-page" className="space-y-[12px]">
      <header className="flex items-end justify-between gap-[12px]">
        <div>
          <h1 className="m-0 text-[length:22px] font-normal tracking-[var(--display-tracking)] text-[color:var(--text)]">
            Crawlers
          </h1>
          <p className="m-0 mt-[2px] text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
            AI-bot hits on {primaryBrand ? <span className="text-[color:var(--text)]">{primaryBrand}</span> : "your site"} from your server logs and CDN, plus crawl-to-referral ratio.
          </p>
        </div>
        <div className="flex items-center border border-[color:var(--border)]">
          {WINDOWS.map((w) => (
            <Link
              key={w}
              href={`/crawlers?days=${w}`}
              aria-current={w === days ? "page" : undefined}
              className={[
                "px-[10px] py-[5px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)]",
                w === days
                  ? "bg-[color:var(--bg-elev-2)] text-[color:var(--text)]"
                  : "text-[color:var(--text-muted)] hover:text-[color:var(--text)]",
              ].join(" ")}
            >
              {w}d
            </Link>
          ))}
        </div>
      </header>

      {!hasBots ? (
        <ConnectSource projectName={primaryBrand} />
      ) : (
        <>
          <Card eyebrow="bots" title="Verified AI-crawler activity">
            <table className="w-full border-collapse text-[length:var(--font-size-sm)]">
              <thead>
                <tr className="border-b border-[color:var(--hairline)] text-left text-[color:var(--text-faint)]">
                  <Th>Bot</Th>
                  <Th right>Hits</Th>
                  <Th right>Verified</Th>
                  <Th right>Errors</Th>
                </tr>
              </thead>
              <tbody>
                {metrics!.bots.map((b) => (
                  <tr key={b.bot_id} className="border-b border-[color:var(--hairline)]">
                    <Td>{b.bot_id}</Td>
                    <Td right>{b.hits}</Td>
                    <Td right>{b.verified_hits}</Td>
                    <Td right danger={b.error_hits > 0}>
                      {b.error_hits}
                    </Td>
                  </tr>
                ))}
              </tbody>
            </table>
          </Card>

          {metrics!.top_paths.length > 0 && (
            <Card eyebrow="paths" title="Top crawled paths">
              <table className="w-full border-collapse text-[length:var(--font-size-sm)]">
                <thead>
                  <tr className="border-b border-[color:var(--hairline)] text-left text-[color:var(--text-faint)]">
                    <Th>Path</Th>
                    <Th right>Hits</Th>
                    <Th right>Errors</Th>
                  </tr>
                </thead>
                <tbody>
                  {metrics!.top_paths.map((p) => (
                    <tr key={p.path} className="border-b border-[color:var(--hairline)]">
                      <Td mono>{p.path}</Td>
                      <Td right>{p.hits}</Td>
                      <Td right danger={p.error_hits > 0}>
                        {p.error_hits}
                      </Td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </Card>
          )}
        </>
      )}

      <Card
        eyebrow="ratio"
        title="Crawl-to-refer ratio"
        accent
        action={
          ratio?.state === "crawls_only" ? (
            <span className="border border-[color:var(--border)] px-[6px] py-[2px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--warn)]">
              crawls-only
            </span>
          ) : undefined
        }
      >
        {ratio?.state === "crawls_only" && (
          <p className="mb-[10px] text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
            Referral attribution is not wired yet, so the ratio reports crawl volume only. Bots
            appear once referral data lands.
          </p>
        )}
        {!ratio || ratio.bots.length === 0 ? (
          <EmptyState title="No crawl-to-refer data" hint="CLI parity: ogeo crawlers --ratio" />
        ) : (
          <table className="w-full border-collapse text-[length:var(--font-size-sm)]">
            <thead>
              <tr className="border-b border-[color:var(--hairline)] text-left text-[color:var(--text-faint)]">
                <Th>Bot</Th>
                <Th right>Verified crawls</Th>
                <Th right>Referrals</Th>
                <Th right>Ratio</Th>
              </tr>
            </thead>
            <tbody>
              {ratio.bots.map((b) => (
                <tr key={b.bot_id} className="border-b border-[color:var(--hairline)]">
                  <Td>{b.bot_id}</Td>
                  <Td right>{b.verified_crawl_hits}</Td>
                  <Td right>{b.attributed_referrals}</Td>
                  <Td right>{b.ratio == null ? "—" : `${b.ratio.toFixed(1)}:1`}</Td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Card>

      {hasBots && <ConnectSource projectName={primaryBrand} />}
    </section>
  );
}

function Th({ children, right }: { children: React.ReactNode; right?: boolean }) {
  return (
    <th
      className={[
        "px-[8px] py-[6px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] font-normal uppercase",
        right ? "text-right" : "",
      ].join(" ")}
    >
      {children}
    </th>
  );
}

function Td({
  children,
  right,
  mono,
  danger,
}: {
  children: React.ReactNode;
  right?: boolean;
  mono?: boolean;
  danger?: boolean;
}) {
  return (
    <td
      className={[
        "px-[8px] py-[6px]",
        right ? "text-right tabular-nums" : "",
        mono ? "font-[family-name:var(--font-mono)]" : "",
        danger ? "text-[color:var(--danger)]" : "text-[color:var(--text)]",
      ].join(" ")}
    >
      {children}
    </td>
  );
}
