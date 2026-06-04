import Link from "next/link";
import { ArrowRight, Filter } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Pill } from "@/components/ui/pill";
import { ProviderDot } from "@/components/ui/provider-dot";
import type {
  AnomalyKind,
  AnomalySeverity,
  AnomalyTimelineProps,
} from "@/lib/api/anomalies";

// Severity → Signal tone token. Higher severity reads as a more alarming dot.
const SEVERITY_TONE: Readonly<Record<AnomalySeverity, string>> = {
  low: "info",
  medium: "warn",
  high: "danger",
};

// Wire-stable anomaly kinds → operator-facing row titles.
const KIND_TITLE: Readonly<Record<AnomalyKind, string>> = {
  visibility_drop: "Visibility dropped",
  citation_loss: "Citation lost",
  rank_swap: "Ranking changed",
};

export type { AnomalyTimelineProps };

export function AnomalyTimeline({ items }: AnomalyTimelineProps) {
  return (
    <Card
      eyebrow="signal timeline"
      title="Anomalies"
      action={
        <>
          <Pill tone="neutral">last 7d</Pill>
          <Button variant="ghost" size="sm" leadingIcon={<Filter size={11} strokeWidth={1.5} />}>
            Filter
          </Button>
        </>
      }
    >
      <div className="flex flex-col">
        {items.map((item, i) => {
          const tone = SEVERITY_TONE[item.severity];
          return (
            <div
              key={item.id}
              className="grid grid-cols-[100px_16px_1fr_auto] items-center gap-[12px] py-[8px]"
              style={{
                borderBottom:
                  i === items.length - 1
                    ? undefined
                    : "1px solid var(--hairline)",
              }}
            >
              <div className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
                {new Date(item.detected_at)
                  .toISOString()
                  .slice(5, 16)
                  .replace("T", " ")}
              </div>
              <div
                className="h-[10px] w-[10px] rounded-full"
                style={{
                  background: `var(--${tone})`,
                  boxShadow: `0 0 0 4px color-mix(in oklch, var(--${tone}) 18%, transparent)`,
                }}
                aria-hidden
              />
              <div className="min-w-0">
                <div className="flex items-center gap-[6px] text-[length:var(--font-size-sm)] text-[color:var(--text)]">
                  {KIND_TITLE[item.kind]}
                  <ProviderDot provider={item.provider} />
                  {item.prompt && <Pill mono>prompt:{item.prompt}</Pill>}
                </div>
                <div className="mt-[2px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-muted)]">
                  {item.provider} · Δ {item.delta.toFixed(2)} over{" "}
                  {item.window_days}d
                </div>
              </div>
              <Link
                href="/runs"
                className="inline-flex items-center gap-[5px] border border-[color:var(--border)] px-[8px] py-[4px] text-[length:var(--font-size-sm)] text-[color:var(--text-muted)] hover:text-[color:var(--text)]"
              >
                Investigate <ArrowRight size={11} strokeWidth={1.5} />
              </Link>
            </div>
          );
        })}
      </div>
    </Card>
  );
}
