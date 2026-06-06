import Link from "next/link";
import { notFound } from "next/navigation";
import { ArrowLeft, Code, RefreshCw } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { PageHeader } from "@/components/ui/page-header";
import { Pill } from "@/components/ui/pill";
import { fetchRunDetail, type RunDetail } from "@/lib/api";
import {
  fetchRunCitations,
  fetchRunMentions,
  fetchRunProvenance,
  fetchRunResponses,
  type RunCitationEntry,
  type RunMentionEntry,
  type RunProvenanceStep,
  type RunResponseEntry,
} from "@/lib/api/run-detail";
import { demoOrEmpty } from "@/lib/data-source";
import type { ProviderId } from "@/lib/provider-colors";

import { LocalTime } from "../../_components/local-time";
import { CitationsPanel } from "./_components/citations-panel";
import { CopyButton } from "./_components/copy-button";
import { MentionsMatrix } from "./_components/mentions-matrix";
import { ProvenancePanel } from "./_components/provenance-panel";
import { RawPanel } from "./_components/raw-panel";
import { ResponseDiff } from "./_components/response-diff";
import { RunDetailTabs } from "./_components/run-detail-tabs";

const KNOWN_PROVIDERS: ReadonlyArray<ProviderId> = [
  "openai",
  "anthropic",
  "gemini",
  "perplexity",
];

/** Demo-mode adapters. Imported lazily so the mock module never reaches the
 *  bundle/render path unless `OGEO_DEMO=1` (see lib/data-source.ts). The mock
 *  is keyed per-provider; we project the run's own provider out of it so the
 *  single-(run,provider) shape of the live endpoints is preserved. */
async function demoFor(provider: string) {
  const { EXTRACTED_MENTIONS, RUN_CITATIONS, SAMPLE_RESPONSES } = await import(
    "@/lib/mock"
  );
  const p = (KNOWN_PROVIDERS as ReadonlyArray<string>).includes(provider)
    ? (provider as ProviderId)
    : "openai";
  // The mock fixtures only model the first-party providers; `p` is always one
  // of them (non-first-party identities fall back to "openai" above), but the
  // fixtures are `Partial`-typed so coalesce to the openai content defensively.
  const fixtureMentions = EXTRACTED_MENTIONS[p] ?? EXTRACTED_MENTIONS.openai ?? [];
  const fixtureResponse = SAMPLE_RESPONSES[p] ?? SAMPLE_RESPONSES.openai ?? "";
  const mentions: RunMentionEntry[] = fixtureMentions.map((m, i) => ({
    id: `demo-mention-${i}`,
    entity: m.brand,
    provider,
    rank: m.rank,
    char_offset: 0,
    matched_text: m.brand,
  }));
  const citations: RunCitationEntry[] = RUN_CITATIONS.map((c, i) => ({
    id: `demo-citation-${i}`,
    domain: c.domain,
    url: c.url,
    source_type: c.type,
    frequency: c.from.length,
    provider,
  }));
  const responses: RunResponseEntry[] = [
    {
      provider,
      provider_model_version: "demo",
      status: "ok",
      raw_response: fixtureResponse,
    },
  ];
  return { mentions, citations, responses };
}

export default async function RunDetailPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = await params;

  let run: RunDetail;
  try {
    run = await fetchRunDetail(id);
  } catch (e) {
    if (e instanceof Error && e.message.includes("404")) notFound();
    return (
      <section data-testid="run-detail-error" className="space-y-2">
        <Link
          href="/runs"
          className="inline-flex items-center gap-[5px] text-[length:var(--font-size-sm)] text-[color:var(--text-muted)] hover:text-[color:var(--text)]"
        >
          <ArrowLeft size={11} strokeWidth={1.5} /> Runs
        </Link>
        <PageHeader title="Run unavailable" />
        <p className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-sm)] text-[color:var(--warn)]">
          {String(e)}
        </p>
      </section>
    );
  }

  // Live per-run extraction data (Story 30-6 endpoints). A failed fetch is
  // treated as "no rows" so a single panel hiccup doesn't blank the page.
  const [liveMentions, liveCitations, liveResponses, liveProvenance] =
    await Promise.all([
      fetchRunMentions(run.id).catch(() => [] as RunMentionEntry[]),
      fetchRunCitations(run.id).catch(() => [] as RunCitationEntry[]),
      fetchRunResponses(run.id).catch(() => [] as RunResponseEntry[]),
      fetchRunProvenance(run.id).catch(() => [] as RunProvenanceStep[]),
    ]);

  // Demo-data contract: live present → render it; empty + demo → mock (with a
  // visible <DemoBadge/>); empty + not demo → the panel renders an EmptyState.
  const mentionsResult = demoOrEmpty(
    liveMentions,
    () => [] as RunMentionEntry[],
  );
  const citationsResult = demoOrEmpty(
    liveCitations,
    () => [] as RunCitationEntry[],
  );
  const responsesResult = demoOrEmpty(
    liveResponses,
    () => [] as RunResponseEntry[],
  );

  let mentions = mentionsResult.data;
  let citations = citationsResult.data;
  let responses = responsesResult.data;
  let isDemo = false;

  if (
    mentionsResult.isDemo ||
    citationsResult.isDemo ||
    responsesResult.isDemo
  ) {
    const demo = await demoFor(run.provider);
    if (mentions.length === 0) mentions = demo.mentions;
    if (citations.length === 0) citations = demo.citations;
    if (responses.length === 0) responses = demo.responses;
    isDemo = true;
  }

  return (
    <section data-testid="run-detail-page" className="flex flex-col gap-[12px]">
      <Card padding={false}>
        <div className="flex items-center justify-between gap-[12px] p-[14px]">
          <div className="min-w-0">
            <div className="flex items-center gap-[8px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
              <Link
                href="/runs"
                className="inline-flex items-center gap-[5px] border border-[color:var(--border)] px-[8px] py-[4px] text-[color:var(--text-muted)] hover:text-[color:var(--text)]"
              >
                <ArrowLeft size={11} strokeWidth={1.5} /> Runs
              </Link>
              <span>
                {run.id} · <LocalTime iso={run.started_at} mode="datetime" />
              </span>
            </div>
            <h1 className="m-0 mt-[6px] text-[length:var(--font-size-2xl)] font-normal tracking-[var(--display-tracking)] text-[color:var(--text)]">{run.prompt_name}</h1>
            <div className="mt-[6px] flex flex-wrap gap-[6px]">
              <Pill mono>prompt: {run.prompt_name}</Pill>
              <Pill mono>provider: {run.provider}</Pill>
              <Pill mono>{run.provider_model_version}</Pill>
              {run.status === "ok" ? (
                <Pill mono tone="ok">
                  ok
                </Pill>
              ) : (
                <Pill mono tone="danger">
                  {run.error_kind ?? "failed"}
                </Pill>
              )}
            </div>
          </div>
          <div className="flex flex-shrink-0 gap-[6px]">
            <CopyButton value={run.id}>Copy run id</CopyButton>
            <Button
              variant="ghost"
              size="sm"
              leadingIcon={<Code size={11} strokeWidth={1.5} />}
            >
              CLI
            </Button>
            <Button
              variant="primary"
              size="sm"
              leadingIcon={<RefreshCw size={11} strokeWidth={1.5} />}
            >
              Re-run
            </Button>
          </div>
        </div>
        <RunDetailTabs
          responseSlot={
            <ResponseDiff
              responses={responses}
              mentions={mentions}
              isDemo={isDemo}
            />
          }
          mentionsSlot={<MentionsMatrix mentions={mentions} isDemo={isDemo} />}
          citationsSlot={
            <CitationsPanel citations={citations} isDemo={isDemo} />
          }
          rawSlot={<RawPanel run={run} />}
          provenanceSlot={<ProvenancePanel steps={liveProvenance} />}
        />
      </Card>
    </section>
  );
}
