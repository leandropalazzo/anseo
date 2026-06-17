"use client";

import { useState } from "react";

import { Card } from "@/components/ui/card";
import { CodeBlock } from "@/components/ui/code-block";
import { ingestCrawlerLogs, type IngestResult } from "@/lib/api";

/**
 * "Connect a crawler source" panel. Two paths, both scoped to the current
 * project: (1) paste access-log lines to ingest right now, (2) point a
 * standing adapter (CLI/log shipper) at the project. Answers "how do I fire
 * this up for my brand" (Epic 31/33 parity gap).
 */
export function ConnectSource({ projectName }: { projectName?: string }) {
  const [text, setText] = useState("");
  const [format, setFormat] = useState<"combined" | "common">("combined");
  const [result, setResult] = useState<IngestResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function ingest() {
    const lines = text.split("\n").map((l) => l.trim()).filter(Boolean);
    if (lines.length === 0) return;
    setBusy(true);
    setError(null);
    setResult(null);
    try {
      setResult(await ingestCrawlerLogs({ lines, format }));
    } catch (e) {
      setError(e instanceof Error ? e.message : "ingest failed");
    } finally {
      setBusy(false);
    }
  }

  return (
    <Card
      eyebrow={projectName ? `connect a source · ${projectName}` : "connect a source"}
      title="Wire crawler hits to this project"
      accent
    >
      <p className="text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
        Crawler metrics populate once AI-bot hits from your own server logs or CDN reach this
        project. Paste a slice of access-log lines to ingest now, or point a standing adapter at
        the project for continuous capture.
      </p>

      <div className="mt-[12px] grid gap-[6px]">
        <div className="flex items-center justify-between">
          <span className="label-eyebrow text-[color:var(--text-faint)]">paste access-log lines</span>
          <select
            value={format}
            onChange={(e) => setFormat(e.target.value as "combined" | "common")}
            className="border border-[color:var(--border)] bg-[color:var(--bg-sunken)] px-[6px] py-[3px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text)]"
            aria-label="log format"
          >
            <option value="combined">combined</option>
            <option value="common">common</option>
          </select>
        </div>
        <textarea
          value={text}
          onChange={(e) => setText(e.target.value)}
          rows={5}
          placeholder={`66.249.66.1 - - [02/Jun/2026:10:00:00 +0000] "GET /docs HTTP/1.1" 200 1024 "-" "Mozilla/5.0 (compatible; GPTBot/1.0)"`}
          className="w-full resize-y border border-[color:var(--border)] bg-[color:var(--bg-sunken)] p-[8px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text)] outline-none focus:border-[color:var(--border-strong)]"
          data-testid="crawler-ingest-textarea"
        />
        <div className="flex items-center gap-[10px]">
          <button
            type="button"
            onClick={ingest}
            disabled={busy || !text.trim()}
            className="border border-[color:var(--border-strong)] bg-[color:var(--accent)] px-[12px] py-[6px] text-[length:var(--font-size-sm)] text-[color:var(--accent-ink)] disabled:opacity-50"
            data-testid="crawler-ingest-button"
          >
            {busy ? "Ingesting…" : "Ingest lines"}
          </button>
          {result && (
            <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-muted)]">
              parsed {result.parsed} · ingested {result.ingested} · skipped {result.skipped}
            </span>
          )}
          {error && (
            <span className="text-[length:var(--font-size-sm)] text-[color:var(--danger)]">{error}</span>
          )}
        </div>
      </div>

      <div className="mt-[14px]">
        <span className="label-eyebrow text-[color:var(--text-faint)]">or run a standing adapter</span>
        <CodeBlock
          lang="bash"
          code={`# continuously tail your access log into this project
ogeo crawlers --config anseo.yaml   # read metrics
# adapters: nginx/Apache logs, Cloudflare Logpush/Workers, Fastly, CloudFront, GA4
# privacy mode defaults to hashed (GDPR-safe self-host)`}
        />
      </div>
    </Card>
  );
}
