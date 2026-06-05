"use client";

import { useState } from "react";
import { Eye } from "lucide-react";

import { DemoBadge } from "@/components/demo-badge";
import { EmptyState } from "@/components/ui/empty-state";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Pill } from "@/components/ui/pill";
import { ProviderDot } from "@/components/ui/provider-dot";
import type {
  RunMentionEntry,
  RunResponseEntry,
} from "@/lib/api/run-detail";
import { resolveProviderIdentity } from "@/lib/provider-colors";

import { HighlightedResponse } from "./highlighted-response";

export interface ResponseDiffProps {
  /** Live responses for this run. A run is a single (run,provider) pair, so
   *  this list has exactly one entry; a true multi-provider diff would need
   *  multiple runs (out of scope) — we render what the endpoint returns. */
  responses: ReadonlyArray<RunResponseEntry>;
  /** Live mentions for this run, used to show the extracted rank list. */
  mentions: ReadonlyArray<RunMentionEntry>;
  /** True when the data is demo data shown under `OGEO_DEMO=1`. */
  isDemo?: boolean;
}

/** Renders a `raw_response` JSON value as displayable text. Prefers a
 *  `text` field if present (the common provider shape), else stringifies. */
function responseText(raw: unknown): string {
  if (typeof raw === "string") return raw;
  if (raw && typeof raw === "object") {
    const t = (raw as Record<string, unknown>).text;
    if (typeof t === "string") return t;
  }
  try {
    return JSON.stringify(raw, null, 2);
  } catch {
    return String(raw);
  }
}

/**
 * Single-provider response panel. A `prompt_runs` row is one (run, provider)
 * pair, so the `/responses` endpoint returns exactly one entry — we render
 * that provider's raw response plus its extracted rank list. The brand row is
 * highlighted with the accent token.
 */
export function ResponseDiff({
  responses,
  mentions,
  isDemo = false,
}: ResponseDiffProps) {
  const [highlight, setHighlight] = useState(true);

  if (responses.length === 0) {
    return (
      <EmptyState
        title="No response captured"
        message="This run has no raw response recorded yet."
      />
    );
  }

  const ranked = [...mentions].sort(
    (a, b) => a.rank - b.rank || a.entity.localeCompare(b.entity),
  );

  return (
    <Card
      padding={false}
      eyebrow="raw response · this run"
      title="Response"
      action={
        <>
          {isDemo && <DemoBadge />}
          <Pill mono>brand: pinecone</Pill>
          <Button
            variant="ghost"
            size="sm"
            leadingIcon={<Eye size={11} strokeWidth={1.5} />}
            onClick={() => setHighlight((v) => !v)}
          >
            {highlight ? "Plain text" : "Highlight"}
          </Button>
        </>
      }
    >
      <div className="border-t border-[color:var(--hairline)]">
        {responses.map((r) => {
          const identity = resolveProviderIdentity(r.provider);
          return (
            <div key={r.provider} className="flex flex-col">
              <div
                className="flex items-center gap-[8px] border-b border-[color:var(--hairline)] px-[12px] py-[8px]"
                style={{
                  background: `color-mix(in oklch, ${identity.cssVar} 8%, transparent)`,
                }}
              >
                <ProviderDot provider={r.provider} />
                <span className="text-[length:var(--font-size-sm)] text-[color:var(--text)]">
                  {identity.label}
                </span>
                <Pill mono>{r.provider_model_version.slice(0, 24)}</Pill>
                <Pill mono tone={r.status === "ok" ? "ok" : "danger"}>
                  {r.status}
                </Pill>
              </div>
              <div className="max-h-[420px] overflow-auto p-[12px] text-[length:var(--font-size-sm)] leading-[1.55] text-[color:var(--text)]">
                <HighlightedResponse
                  text={responseText(r.raw_response)}
                  highlight={highlight}
                />
              </div>
              <div className="border-t border-[color:var(--hairline)] bg-[color:var(--bg-sunken)] p-[10px]">
                <div className="mb-[4px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
                  extracted rank
                </div>
                {ranked.length === 0 ? (
                  <div className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
                    no mentions extracted
                  </div>
                ) : (
                  <div className="flex flex-col gap-[2px]">
                    {ranked.slice(0, 8).map((m) => {
                      const ours = m.entity.toLowerCase() === "pinecone";
                      return (
                        <div
                          key={m.id}
                          className="grid grid-cols-[20px_1fr_28px] items-center gap-[6px] px-[4px] py-[2px]"
                          style={{
                            background: ours
                              ? "var(--accent-soft)"
                              : "transparent",
                          }}
                        >
                          <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
                            {m.rank}.
                          </span>
                          <span
                            className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)]"
                            style={{
                              color: ours ? "var(--text)" : "var(--text-muted)",
                              fontWeight: ours ? 600 : 400,
                            }}
                          >
                            {m.entity}
                          </span>
                          {ours && (
                            <Pill tone="accent" mono>
                              brand
                            </Pill>
                          )}
                        </div>
                      );
                    })}
                  </div>
                )}
              </div>
            </div>
          );
        })}
      </div>
    </Card>
  );
}
