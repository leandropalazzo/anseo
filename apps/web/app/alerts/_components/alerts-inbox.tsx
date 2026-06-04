"use client";

import type { CSSProperties } from "react";
import { AlertTriangle, Check, Sparkles, TrendingUp } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Pill } from "@/components/ui/pill";
import { ProviderDot } from "@/components/ui/provider-dot";
import { EmptyState } from "@/components/ui/empty-state";
import { ICON_DEFAULTS } from "@/lib/icons";
import type {
  AnomalyItem,
  AnomalyKind,
  AnomalySeverity,
} from "@/lib/api/anomalies";

/** Inbox severity glyph levels, mapped from the wire `AnomalySeverity`. */
type Level = "info" | "ok" | "warn" | "danger";

const LEVEL_VAR: Readonly<Record<Level, string>> = {
  danger: "--danger",
  warn: "--warn",
  ok: "--ok",
  info: "--info",
};

/** `AnomalyItem.severity` → inbox `level`/glyph. */
const SEVERITY_LEVEL: Readonly<Record<AnomalySeverity, Level>> = {
  high: "danger",
  medium: "warn",
  low: "info",
};

/** Human-readable title per `AnomalyItem.kind`. */
const KIND_TITLE: Readonly<Record<AnomalyKind, string>> = {
  visibility_drop: "Visibility dropped",
  citation_loss: "Citation loss",
  rank_swap: "Ranking swap",
};

function LevelGlyph({ level }: { level: Level }) {
  const size = 14;
  const sw = ICON_DEFAULTS.strokeWidth;
  if (level === "danger") return <AlertTriangle size={size} strokeWidth={sw} />;
  if (level === "warn") return <TrendingUp size={size} strokeWidth={sw} />;
  if (level === "ok") return <Check size={size} strokeWidth={sw} />;
  return <Sparkles size={size} strokeWidth={sw} />;
}

/** Best-effort detail line from the opaque delta signal. */
function detailLine(a: AnomalyItem): string {
  const target = a.prompt ? `'${a.prompt}'` : "across prompts";
  const where = a.provider === "*" ? "all providers" : a.provider;
  return `${KIND_TITLE[a.kind]} on ${where} for ${target} over ${a.window_days}d (Δ ${a.delta}).`;
}

export interface AlertsInboxProps {
  incidents: ReadonlyArray<AnomalyItem>;
}

export function AlertsInbox({ incidents }: AlertsInboxProps) {
  if (incidents.length === 0) {
    return (
      <div className="p-[14px]">
        <EmptyState
          icon={Check}
          title="No open alerts."
          hint="Anomalies detected over the last 7d will surface here."
        />
      </div>
    );
  }
  return (
    <div>
      {incidents.map((a, i) => {
        const level = SEVERITY_LEVEL[a.severity];
        const v = LEVEL_VAR[level];
        const glyphStyle: CSSProperties = {
          background: `color-mix(in oklch, var(${v}) 14%, transparent)`,
          border: `1px solid color-mix(in oklch, var(${v}) 40%, transparent)`,
          color: `var(${v})`,
        };
        return (
          <div
            key={a.id}
            className="grid grid-cols-[auto_1fr_auto] items-center gap-[14px] px-[14px] py-[12px]"
            style={{
              borderBottom:
                i === incidents.length - 1
                  ? "0"
                  : "1px solid var(--hairline)",
            }}
            data-testid={`alert-${a.id}`}
          >
            <div
              className="inline-flex h-[32px] w-[32px] items-center justify-center"
              style={glyphStyle}
            >
              <LevelGlyph level={level} />
            </div>
            <div>
              <div className="flex flex-wrap items-center gap-[8px]">
                <span className="text-[length:var(--font-size-base)] text-[color:var(--text)]">
                  {KIND_TITLE[a.kind]}
                </span>
                {a.prompt && <Pill mono>prompt:{a.prompt}</Pill>}
                {a.provider !== "*" && (
                  <Pill mono>
                    <ProviderDot provider={a.provider} />
                    <span className="ml-[4px]">{a.provider}</span>
                  </Pill>
                )}
                <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
                  {new Date(a.detected_at)
                    .toISOString()
                    .slice(5, 16)
                    .replace("T", " ")}
                </span>
              </div>
              <div className="mt-[4px] text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
                {detailLine(a)}
              </div>
            </div>
            <div className="flex gap-[6px]">
              <Button size="sm" variant="ghost">
                Snooze
              </Button>
              <Button size="sm" variant="ghost">
                Ack
              </Button>
              <Button size="sm" variant="primary">
                Investigate
              </Button>
            </div>
          </div>
        );
      })}
    </div>
  );
}
