import type { DayCount } from "@/lib/api";
import { chartColor } from "@/lib/chart-colors";

/**
 * Compact day-series bar chart used for the sessions-per-day sparkline and the
 * daily badge-embed serves. Pure SVG, no charting library (Epic 46.1 chart
 * ramp). Bars are evenly spaced; height is proportional to the day's count
 * against the series max. Empty series renders the caller's empty-state, not
 * here.
 */
export function SparkBars({
  data,
  colorIndex = 0,
  height = 96,
  label,
}: {
  data: DayCount[];
  colorIndex?: number;
  height?: number;
  label: string;
}) {
  const max = Math.max(1, ...data.map((d) => d.count));
  const total = data.reduce((acc, d) => acc + d.count, 0);
  const gap = 2;
  const barWidth = 100 / Math.max(1, data.length);
  return (
    <div className="flex flex-col gap-[6px]" data-testid="spark-bars">
      <svg
        viewBox={`0 0 100 ${height}`}
        preserveAspectRatio="none"
        className="w-full"
        style={{ height }}
        role="img"
        aria-label={`${label}: ${total.toLocaleString()} total over ${data.length} days`}
      >
        {data.map((d, i) => {
          const h = (d.count / max) * (height - 4);
          return (
            <rect
              key={d.date}
              x={i * barWidth + gap / 2}
              y={height - h}
              width={Math.max(0.5, barWidth - gap)}
              height={h}
              fill={chartColor(colorIndex)}
            >
              <title>{`${d.date}: ${d.count.toLocaleString()}`}</title>
            </rect>
          );
        })}
      </svg>
      <div className="flex items-center justify-between font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
        <span>{data[0]?.date ?? ""}</span>
        <span className="text-[color:var(--text-muted)]">
          {total.toLocaleString()} total
        </span>
        <span>{data[data.length - 1]?.date ?? ""}</span>
      </div>
    </div>
  );
}
