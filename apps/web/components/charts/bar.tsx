import type { CSSProperties } from "react";

export interface BarProps {
  value: number;
  /** Denominator for `value / max` (default 1). */
  max?: number;
  /** Foreground color (CSS color, accepts `var(--…)`). */
  color?: string;
  height?: number;
  /** Track color. */
  bg?: string;
  ariaLabel?: string;
  className?: string;
}

/**
 * Horizontal progress bar — used as inline meters inside table rows and
 * stat tiles. Ported from `shell.jsx::Bar`. Pill-shaped on purpose
 * (the only rounded geometry in the Signal direction; the brutalist zero
 * radius is reserved for surfaces).
 */
export function Bar({
  value,
  max = 1,
  color = "var(--accent)",
  height = 4,
  bg = "var(--hairline)",
  ariaLabel,
  className,
}: BarProps) {
  const pct = Math.max(0, Math.min(100, (value / (max || 1)) * 100));
  const trackStyle: CSSProperties = {
    height,
    background: bg,
  };
  const fillStyle: CSSProperties = {
    width: `${pct}%`,
    height: "100%",
    background: color,
  };
  return (
    <div
      className={["w-full overflow-hidden rounded-full", className]
        .filter(Boolean)
        .join(" ")}
      role="progressbar"
      aria-label={ariaLabel ?? "progress"}
      aria-valuenow={pct}
      aria-valuemin={0}
      aria-valuemax={100}
      style={trackStyle}
    >
      <div className="rounded-full" style={fillStyle} />
    </div>
  );
}
