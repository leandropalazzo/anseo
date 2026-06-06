/**
 * Anseo lockup using the canonical Anseo mark: a measurement reticle — precise
 * zero-radius frame, reticle ticks, a core node, a tilted orbit, and a sighting
 * node. Reads as instrument / observation, not award. Works at 24px.
 *
 * Source of truth: the delivered brand asset `public/anseo-mark.svg`. The
 * geometry below is byte-faithful to that file; it is kept inline (rather than
 * `<img src>`) so the mark inherits live theme tokens — `var(--accent)` /
 * `var(--accent-ink)` resolve to `#EBD200` / `#0A0A0A`, matching the asset, and
 * recolor correctly under light/dark.
 */
export function Logo({ markOnly = false }: { markOnly?: boolean }) {
  return (
    <span className="inline-flex min-w-0 items-center gap-[8px]">
      <svg
        width="24"
        height="24"
        viewBox="0 0 24 24"
        className="block shrink-0"
        {...(markOnly ? { role: "img" } : { "aria-hidden": true })}
      >
        {/* In mark-only mode the wordmark is hidden, so the SVG must carry the
            accessible name; with the wordmark the text provides it. */}
        {markOnly && <title>Anseo</title>}
        <rect x="0.5" y="0.5" width="23" height="23" fill="var(--accent)" />
        <g stroke="var(--accent-ink)" strokeWidth="1.4">
          <line x1="12" y1="1.6" x2="12" y2="4.2" />
          <line x1="12" y1="19.8" x2="12" y2="22.4" />
          <line x1="1.6" y1="12" x2="4.2" y2="12" />
          <line x1="19.8" y1="12" x2="22.4" y2="12" />
        </g>
        <g fill="none" stroke="var(--accent-ink)" strokeWidth="1.5">
          <circle cx="12" cy="12" r="3.1" fill="var(--accent-ink)" stroke="none" />
          <ellipse cx="12" cy="12" rx="8.4" ry="4.1" transform="rotate(-30 12 12)" />
          <circle cx="18.7" cy="8.4" r="1.85" fill="var(--accent-ink)" stroke="none" />
        </g>
      </svg>
      {!markOnly && (
        <span className="truncate font-[family-name:var(--font-display)] text-[length:15px] font-semibold tracking-[var(--display-tracking)] text-[color:var(--text)]">
          Anseo
        </span>
      )}
    </span>
  );
}
