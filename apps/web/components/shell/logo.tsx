/**
 * OpenGEO lockup using the Claude Design "Orbit" mark.
 *
 * The mark is built from the design's 24x24 primitives: a core, tilted orbit,
 * and provider node. It stays inline so the app shell can inherit tokens.
 */
export function Logo({ markOnly = false }: { markOnly?: boolean }) {
  return (
    <span className="inline-flex min-w-0 items-center gap-[8px]" aria-label="OpenGEO">
      <svg
        width="24"
        height="24"
        viewBox="0 0 24 24"
        className="block shrink-0"
        aria-hidden="true"
      >
        <rect
          x="0.5"
          y="0.5"
          width="23"
          height="23"
          rx="2"
          fill="var(--accent)"
        />
        <g fill="none" stroke="var(--accent-ink)" strokeWidth="1.6">
          <circle cx="12" cy="12" r="3.2" fill="var(--accent-ink)" stroke="none" />
          <ellipse cx="12" cy="12" rx="9" ry="4.4" transform="rotate(-30 12 12)" />
          <circle cx="19.4" cy="8.1" r="2" fill="var(--accent-ink)" stroke="none" />
        </g>
      </svg>
      {!markOnly && (
        <span className="truncate font-[family-name:var(--font-display)] text-[length:15px] font-semibold tracking-[var(--display-tracking)] text-[color:var(--text)]">
          OpenGEO
        </span>
      )}
    </span>
  );
}
