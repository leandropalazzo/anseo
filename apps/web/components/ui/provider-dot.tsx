import type { CSSProperties } from "react";

import { resolveProviderIdentity } from "@/lib/provider-colors";

export interface ProviderDotProps {
  /**
   * Provider identity. Accepts a plain wire name (`openai`, …) or an
   * OpenRouter identity string (`openrouter:<upstream>`); resolved through
   * `resolveProviderIdentity` so distinct upstreams render with a color +
   * label and unknown ids fall back gracefully.
   */
  provider: string;
  /** Diameter in px (default 8). */
  size?: number;
  /** Render the provider label alongside the dot. */
  withLabel?: boolean;
  /** Dim opacity for "off" / "muted" rows. */
  dim?: boolean;
  className?: string;
}

/**
 * Provider identity glyph.
 *
 * A11y rule (UX-DR): the color dot is ALWAYS paired with the `█` density
 * glyph (rendered as a visually-hidden text node for screen readers and
 * shown as a tooltip-ready text label). Colorblind operators rely on the
 * label + glyph combination. Do not strip these.
 */
export function ProviderDot({
  provider,
  size = 8,
  withLabel = false,
  dim = false,
  className,
}: ProviderDotProps) {
  const meta = resolveProviderIdentity(provider);
  const color = meta.cssVar;
  const dotStyle: CSSProperties = {
    width: size,
    height: size,
    background: color,
    boxShadow: `0 0 0 1px color-mix(in oklch, ${color} 30%, transparent)`,
    color: "var(--bg)",
    fontSize: Math.max(7, Math.floor(size * 0.56)),
    lineHeight: `${size}px`,
  };
  return (
    <span
      className={[
        "inline-flex items-center gap-[6px]",
        dim ? "opacity-55" : "",
        className,
      ]
        .filter(Boolean)
        .join(" ")}
      aria-label={meta.label}
      title={meta.label}
    >
      <span
        aria-hidden
        className="inline-flex shrink-0 items-center justify-center rounded-full font-[family-name:var(--font-mono)] font-semibold"
        style={dotStyle}
      >
        {meta.iconPath ? (
          <svg
            viewBox="0 0 24 24"
            aria-hidden
            width={Math.max(6, Math.round(size * 0.66))}
            height={Math.max(6, Math.round(size * 0.66))}
            className="block"
          >
            <path d={meta.iconPath} fill="currentColor" />
          </svg>
        ) : size >= 10 ? (
          meta.logo
        ) : null}
      </span>
      {/* Density glyph — visible for sighted users when `withLabel`, and
       * announced via the aria-label above so colorblind operators always
       * get the textual signal regardless of layout. */}
      <span className="sr-only">{meta.glyph}</span>
      {withLabel && (
        <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text)]">
          {meta.label}
        </span>
      )}
    </span>
  );
}
