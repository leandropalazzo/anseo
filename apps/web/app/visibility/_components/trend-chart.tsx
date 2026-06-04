import { resolveProviderIdentity } from "@/lib/provider-colors";

export type TrendMetric = "rank" | "presence";

/** One trend datapoint per bucket for a single provider. Mirrors the live
 *  `VisibilityPoint` shape from `@/lib/api`, with `avg_rank` coerced to a
 *  number for charting (`null` ranks are rendered as 0). The `provider` is a
 *  provider identity string, resolved for color/label via
 *  `resolveProviderIdentity`. */
export interface TrendPoint {
  bucket_start: string;
  provider: string;
  avg_rank: number;
  presence_rate: number;
}

export interface TrendSeries {
  provider: string;
  points: ReadonlyArray<TrendPoint>;
}

export interface TrendChartProps {
  series: ReadonlyArray<TrendSeries>;
  metric: TrendMetric;
  /** Render an anomaly band over the last N points. */
  anomalyBandTrailing?: number | null;
}

/**
 * Multi-line trend chart. Hand-rolled SVG, no chart libs. One line per
 * provider, optional dashed anomaly band on the trailing window.
 *
 * Rank axis is "lower is better"; we invert so up = good on the chart.
 */
export function TrendChart({
  series,
  metric,
  anomalyBandTrailing = null,
}: TrendChartProps) {
  const w = 900;
  const h = 280;
  const padL = 50;
  const padR = 70;
  const padT = 16;
  const padB = 28;
  const xs = series[0]?.points.length ?? 0;
  if (xs < 2) {
    return (
      <div className="px-[14px] py-[24px] text-center font-[family-name:var(--font-mono)] text-[length:var(--font-size-sm)] text-[color:var(--text-faint)]">
        not enough data
      </div>
    );
  }
  const xStep = (w - padL - padR) / (xs - 1);

  const allValues = series.flatMap((s) =>
    s.points.map((p) => (metric === "rank" ? p.avg_rank : p.presence_rate)),
  );
  let min = Math.min(...allValues);
  let max = Math.max(...allValues);
  if (metric === "presence") {
    min = 0;
    max = 1;
  } else {
    min = Math.max(1, Math.floor(min - 0.5));
    max = Math.ceil(max + 0.5);
  }
  const span = max - min || 1;

  const yScale = (v: number) => {
    const norm = (v - min) / span;
    return metric === "rank"
      ? padT + norm * (h - padT - padB)
      : padT + (1 - norm) * (h - padT - padB);
  };

  const labels = series[0]!.points.map((p) => p.bucket_start.slice(5, 10));
  const lblIdx = Array.from({ length: 5 }, (_, i) =>
    Math.floor((i * (xs - 1)) / 4),
  );

  const anomalyBand =
    anomalyBandTrailing && anomalyBandTrailing > 0 && xs > anomalyBandTrailing
      ? { start: xs - anomalyBandTrailing, end: xs - 1 }
      : null;

  return (
    <svg
      role="img"
      aria-label={`Visibility trend — ${metric}`}
      viewBox={`0 0 ${w} ${h}`}
      className="block w-full"
    >
      {/* gridlines + Y labels */}
      {Array.from({ length: 5 }, (_, i) => {
        const y = padT + (i * (h - padT - padB)) / 4;
        const val =
          metric === "rank"
            ? (min + (span * i) / 4).toFixed(1)
            : `${((1 - i / 4) * 100).toFixed(0)}%`;
        return (
          <g key={i}>
            <line
              x1={padL}
              x2={w - padR}
              y1={y}
              y2={y}
              stroke="var(--hairline)"
              strokeWidth={1}
            />
            <text
              x={padL - 8}
              y={y + 4}
              textAnchor="end"
              fill="var(--text-faint)"
              style={{ fontFamily: "var(--font-mono)", fontSize: 10 }}
            >
              {val}
            </text>
          </g>
        );
      })}

      {/* anomaly band */}
      {anomalyBand && (
        <g>
          <rect
            x={padL + anomalyBand.start * xStep}
            y={padT}
            width={(anomalyBand.end - anomalyBand.start) * xStep}
            height={h - padT - padB}
            fill="color-mix(in oklch, var(--danger) 8%, transparent)"
            stroke="color-mix(in oklch, var(--danger) 30%, transparent)"
            strokeDasharray="2 3"
          />
          <text
            x={
              padL +
              (anomalyBand.start +
                (anomalyBand.end - anomalyBand.start) / 2) *
                xStep
            }
            y={padT + 12}
            textAnchor="middle"
            fill="var(--danger)"
            style={{
              fontFamily: "var(--font-mono)",
              fontSize: 10,
              textTransform: "uppercase",
              letterSpacing: 0.5,
            }}
          >
            ranking drop · {anomalyBandTrailing}d
          </text>
        </g>
      )}

      {/* X labels */}
      {lblIdx.map((i, pos) => (
        <text
          key={`${pos}-${i}`}
          x={padL + i * xStep}
          y={h - 8}
          textAnchor="middle"
          fill="var(--text-faint)"
          style={{ fontFamily: "var(--font-mono)", fontSize: 10 }}
        >
          {labels[i]}
        </text>
      ))}

      {/* lines */}
      {series.map((s) => {
        const identity = resolveProviderIdentity(s.provider);
        const color = identity.cssVar;
        const path = s.points
          .map((p, i) => {
            const v = metric === "rank" ? p.avg_rank : p.presence_rate;
            return `${i === 0 ? "M" : "L"}${padL + i * xStep},${yScale(v)}`;
          })
          .join(" ");
        const last = s.points[s.points.length - 1]!;
        const lastV = metric === "rank" ? last.avg_rank : last.presence_rate;
        const dotEvery = Math.max(1, Math.floor(xs / 12));
        return (
          <g key={s.provider}>
            <path
              d={path}
              fill="none"
              stroke={color}
              strokeWidth={1.5}
              strokeLinejoin="round"
              strokeLinecap="round"
            />
            {s.points.map((p, i) => {
              if (i % dotEvery !== 0 && i !== xs - 1) return null;
              const v = metric === "rank" ? p.avg_rank : p.presence_rate;
              return (
                <circle
                  key={i}
                  cx={padL + i * xStep}
                  cy={yScale(v)}
                  r={1.5}
                  fill={color}
                />
              );
            })}
            <text
              x={padL + (xs - 1) * xStep + 6}
              y={yScale(lastV) + 3}
              fill={color}
              style={{ fontFamily: "var(--font-mono)", fontSize: 10 }}
            >
              {identity.label}
            </text>
          </g>
        );
      })}
    </svg>
  );
}
