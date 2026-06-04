import type { CSSProperties, ReactNode } from "react";

export type StatTileDeltaTone = "neutral" | "ok" | "warn" | "danger";

export interface StatTileProps {
  label: ReactNode;
  value: ReactNode;
  delta?: ReactNode;
  deltaTone?: StatTileDeltaTone;
  sparkline?: ReactNode;
  /** Render the value glyph in `--font-mono` (default uses `--font-display`). */
  mono?: boolean;
  /** Bumps the value type scale from 28→40px. */
  big?: boolean;
  className?: string;
}

const DELTA_COLOR: Readonly<Record<StatTileDeltaTone, string>> = {
  neutral: "var(--text-faint)",
  ok: "var(--ok)",
  warn: "var(--warn)",
  danger: "var(--danger)",
};

/**
 * Headline metric tile — label, value, optional delta + sparkline.
 * Used in the Overview grid and as filter chips inside Card headers.
 */
export function StatTile({
  label,
  value,
  delta,
  deltaTone = "neutral",
  sparkline,
  mono = false,
  big = false,
  className,
}: StatTileProps) {
  const valueStyle: CSSProperties = {
    fontFamily: mono ? "var(--font-mono)" : "var(--font-display)",
    fontSize: big ? 40 : 28,
    fontWeight: mono ? 500 : 400,
    letterSpacing: "var(--display-tracking)",
  };
  return (
    <div
      className={[
        "relative overflow-hidden",
        "border border-[color:var(--border)] bg-[color:var(--bg-elev)]",
        "p-[14px]",
        className,
      ]
        .filter(Boolean)
        .join(" ")}
    >
      <div className="label-eyebrow flex items-center justify-between text-[color:var(--text-faint)]">
        <span>{label}</span>
        {delta != null && (
          <span
            className="font-[family-name:var(--font-mono)]"
            style={{ color: DELTA_COLOR[deltaTone] }}
          >
            {delta}
          </span>
        )}
      </div>
      <div
        className="mt-[6px] leading-none text-[color:var(--text)]"
        style={valueStyle}
      >
        {value}
      </div>
      {sparkline && (
        <div data-testid="stat-tile-sparkline" className="mt-[10px]">
          {sparkline}
        </div>
      )}
    </div>
  );
}
