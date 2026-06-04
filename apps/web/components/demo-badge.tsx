/**
 * "DEMO DATA" pill — shown on any surface that is rendering mock data because
 * the dashboard launched in demo mode (`OGEO_DEMO=1`, see `lib/data-source.ts`).
 * Deliberately high-contrast (warn tone) so it's obvious the figures aren't
 * live. Matches the Signal design tokens used across `app/_components`.
 */
export function DemoBadge() {
  return (
    <span
      className="inline-flex items-center gap-1 whitespace-nowrap px-[6px] py-[1px] text-[length:var(--font-size-xs)] uppercase tracking-[0.08em] leading-[1.4]"
      style={{
        border: "1px solid color-mix(in oklch, var(--warn) 40%, transparent)",
        background: "color-mix(in oklch, var(--warn) 12%, transparent)",
        color: "var(--warn)",
        fontFamily: "var(--font-mono)",
      }}
    >
      Demo Data
    </span>
  );
}
