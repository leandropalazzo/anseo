import { resolveProviderIdentity } from "@/lib/provider-colors";
import { ProviderDot } from "@/components/ui/provider-dot";

export interface HeatmapInputRow {
  /** Provider-identity string (plain wire name or `openrouter:<upstream>`). */
  provider: string;
  /** One cell per day, oldest → newest. `null` = brand absent. */
  ranks: ReadonlyArray<number | null>;
}

export interface RankHeatmapProps {
  rows: ReadonlyArray<HeatmapInputRow>;
}

function toneFor(rank: number): "ok" | "info" | "warn" | "danger" {
  if (rank <= 2) return "ok";
  if (rank <= 4) return "info";
  if (rank <= 6) return "warn";
  return "danger";
}

function intensityFor(rank: number): number {
  if (rank <= 2) return 0.85;
  if (rank <= 4) return 0.65;
  if (rank <= 6) return 0.5;
  return 0.35;
}

/**
 * One row per provider × N day-cells. UX-DR: pair the color signal with
 * the `█` density glyph so colorblind operators have a non-color channel.
 * Each cell carries the glyph as `aria-label` and visible text on hover.
 *
 * Closes the visibility-heatmap AC for Story 14.3 (the live data comes
 * from `/v1/analytics/heatmap`; this component just renders cells).
 */
export function RankHeatmap({ rows }: RankHeatmapProps) {
  const cellCount = rows[0]?.ranks.length ?? 0;
  return (
    <div className="flex flex-col gap-[6px]">
      {rows.map((r) => (
        <div
          key={r.provider}
          className="grid items-center gap-[10px]"
          style={{ gridTemplateColumns: "110px 1fr" }}
        >
          <div className="flex items-center gap-[6px]">
            <ProviderDot provider={r.provider} />
            <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-muted)]">
              {resolveProviderIdentity(r.provider).label}
            </span>
          </div>
          <div
            className="grid gap-[2px]"
            style={{ gridTemplateColumns: `repeat(${cellCount}, 1fr)` }}
          >
            {r.ranks.map((rank, i) => {
              if (rank === null) {
                return (
                  <div
                    key={i}
                    title={`day ${i + 1} · absent`}
                    aria-label="absent"
                    className="flex h-[18px] items-center justify-center font-[family-name:var(--font-mono)] text-[8px] text-[color:var(--text-faint)]"
                    style={{
                      background:
                        "color-mix(in oklch, var(--text-faint) 6%, transparent)",
                    }}
                  >
                    ·
                  </div>
                );
              }
              const tone = toneFor(rank);
              const intensity = intensityFor(rank);
              return (
                <div
                  key={i}
                  title={`day ${i + 1} · rank ${rank.toFixed(1)}`}
                  aria-label={`rank ${rank.toFixed(1)} (${tone})`}
                  className="flex h-[18px] items-center justify-center font-[family-name:var(--font-mono)] text-[8px] leading-none"
                  style={{
                    background: `color-mix(in oklch, var(--${tone}) ${
                      intensity * 100
                    }%, transparent)`,
                    color: `color-mix(in oklch, var(--${tone}) 80%, var(--text))`,
                  }}
                >
                  █
                </div>
              );
            })}
          </div>
        </div>
      ))}
      <div className="mt-[4px] flex justify-between font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
        <span>{cellCount}d ago</span>
        <span>today</span>
      </div>
    </div>
  );
}
