import { ProviderDot } from "@/components/ui/provider-dot";
import { Icon } from "@/lib/icons";

export type DeltaDirection = "up" | "down" | "flat";

/** A single winners/losers row. `was`/`now` are avg-rank values (lower is
 *  better), `delta = now - was`; `dir` is "up" when rank improved (delta<0). */
export interface DeltaLeaderboardRow {
  name: string;
  /** Provider identity string; legacy routed identities are normalized in UI. */
  provider: string;
  delta: number;
  was: number;
  now: number;
  dir: DeltaDirection;
}

export interface DeltaLeaderboardProps {
  rows: ReadonlyArray<DeltaLeaderboardRow>;
}

export function DeltaLeaderboard({ rows }: DeltaLeaderboardProps) {
  return (
    <div className="flex flex-col gap-[6px]">
      {rows.map((r, i) => {
        const dirColor =
          r.dir === "up"
            ? "var(--ok)"
            : r.dir === "down"
              ? "var(--danger)"
              : "var(--text-faint)";
        const deltaColor =
          r.delta > 0
            ? "var(--danger)"
            : r.delta < 0
              ? "var(--ok)"
              : "var(--text-faint)";
        const DirIcon =
          r.dir === "up"
            ? Icon.TrendDown
            : r.dir === "down"
              ? Icon.Trend
              : Icon.ArrowRight;
        return (
          <div
            key={`${r.name}-${r.provider}-${i}`}
            className="grid items-center gap-[10px] border border-[color:var(--hairline)] bg-[color:var(--bg-elev-2)] px-[8px] py-[6px]"
            style={{ gridTemplateColumns: "16px 1fr 70px" }}
          >
            <span className="inline-flex" style={{ color: dirColor }}>
              <DirIcon size={12} strokeWidth={1.5} />
            </span>
            <div className="min-w-0">
              <div className="flex items-center gap-[6px]">
                <ProviderDot provider={r.provider} />
                <span className="text-[length:var(--font-size-sm)] text-[color:var(--text)]">
                  {r.name}
                </span>
              </div>
              <div className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
                {r.was.toFixed(1)} → {r.now.toFixed(1)}
              </div>
            </div>
            <span
              className="text-right font-[family-name:var(--font-mono)] text-[length:var(--font-size-sm)]"
              style={{ color: deltaColor }}
            >
              {r.delta > 0 ? "+" : ""}
              {r.delta.toFixed(1)}
            </span>
          </div>
        );
      })}
    </div>
  );
}
