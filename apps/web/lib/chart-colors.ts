/**
 * Shared categorical chart ramp.
 *
 * Single source of truth for multi-series chart colors so no component
 * hand-rolls its own palette. Each entry is a `var(--chart-N)` reference
 * resolved against `apps/web/styles/tokens.css` (light + dark both defined).
 *
 * A11y (UX-DR): color alone is never the only signal — callers MUST also
 * label each band/series (text label or glyph) for color-blind operators.
 */

/** Ordered ramp of `var(...)` strings, ready for inline `style` / SVG fill. */
export const CHART_RAMP: ReadonlyArray<string> = [
  "var(--chart-1)",
  "var(--chart-2)",
  "var(--chart-3)",
  "var(--chart-4)",
  "var(--chart-5)",
  "var(--chart-6)",
  "var(--chart-7)",
];

/** Returns the ramp color for series index `i`, wrapping past the end. */
export function chartColor(i: number): string {
  return CHART_RAMP[((i % CHART_RAMP.length) + CHART_RAMP.length) % CHART_RAMP.length]!;
}
