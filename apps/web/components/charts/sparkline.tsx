import { useId } from "react";

export interface SparklineProps {
  points: ReadonlyArray<number>;
  color?: string;
  height?: number;
  width?: number;
  /** Render an area fill beneath the line. */
  fill?: boolean;
  stroke?: number;
  ariaLabel?: string;
  dataTestId?: string;
}

/**
 * Hand-rolled SVG sparkline. Decision: stay off Recharts / D3 for these
 * micro-charts (matches the density aesthetic, zero bundle cost).
 * Ported from `shell.jsx::Sparkline`.
 */
export function Sparkline({
  points,
  color = "var(--accent)",
  height = 28,
  width = 120,
  fill = true,
  stroke = 1.25,
  ariaLabel,
  dataTestId,
}: SparklineProps) {
  const id = useId();
  if (!points || points.length < 2) return null;

  const min = Math.min(...points);
  const max = Math.max(...points);
  const span = max - min || 1;
  const step = width / (points.length - 1);

  const xy = points.map<[number, number]>((p, i) => [
    i * step,
    height - ((p - min) / span) * height,
  ]);
  const path = xy
    .map(([x, y], i) => `${i === 0 ? "M" : "L"}${x.toFixed(1)},${y.toFixed(1)}`)
    .join(" ");
  const area = fill ? `${path} L${width},${height} L0,${height} Z` : null;
  const gradientId = `sparkline-${id.replace(/:/g, "")}`;

  return (
    <svg
      data-testid={dataTestId}
      width={width}
      height={height}
      role={ariaLabel ? "img" : "presentation"}
      aria-label={ariaLabel}
      style={{ overflow: "visible", display: "block" }}
    >
      {fill && (
        <defs>
          <linearGradient id={gradientId} x1="0" x2="0" y1="0" y2="1">
            <stop offset="0%" stopColor={color} stopOpacity={0.28} />
            <stop offset="100%" stopColor={color} stopOpacity={0} />
          </linearGradient>
        </defs>
      )}
      {fill && area && <path d={area} fill={`url(#${gradientId})`} />}
      <path
        d={path}
        fill="none"
        stroke={color}
        strokeWidth={stroke}
        strokeLinejoin="round"
        strokeLinecap="round"
      />
    </svg>
  );
}
