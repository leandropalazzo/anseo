import type { CSSProperties, ReactNode } from "react";

export type PillTone =
  | "neutral"
  | "ok"
  | "warn"
  | "danger"
  | "info"
  | "accent";

export interface PillProps {
  children: ReactNode;
  tone?: PillTone;
  /** Solid fills the chip with the tone color; default is soft. */
  solid?: boolean;
  /** Render the label in `--font-mono` (default) vs `--font-body`. */
  mono?: boolean;
  className?: string;
}

interface ToneSpec {
  fg: string;
  bg: string;
  bd: string;
}

const TONES: Readonly<Record<PillTone, ToneSpec>> = {
  neutral: {
    fg: "var(--text-muted)",
    bg: "color-mix(in oklch, var(--bg-elev-2) 80%, transparent)",
    bd: "var(--border)",
  },
  ok: {
    fg: "var(--ok)",
    bg: "color-mix(in oklch, var(--ok) 12%, transparent)",
    bd: "color-mix(in oklch, var(--ok) 40%, transparent)",
  },
  warn: {
    fg: "var(--warn)",
    bg: "color-mix(in oklch, var(--warn) 12%, transparent)",
    bd: "color-mix(in oklch, var(--warn) 40%, transparent)",
  },
  danger: {
    fg: "var(--danger)",
    bg: "color-mix(in oklch, var(--danger) 12%, transparent)",
    bd: "color-mix(in oklch, var(--danger) 40%, transparent)",
  },
  info: {
    fg: "var(--info)",
    bg: "color-mix(in oklch, var(--info) 12%, transparent)",
    bd: "color-mix(in oklch, var(--info) 40%, transparent)",
  },
  accent: {
    fg: "var(--accent)",
    bg: "var(--accent-soft)",
    bd: "color-mix(in oklch, var(--accent) 40%, transparent)",
  },
};

export function Pill({
  children,
  tone = "neutral",
  solid = false,
  mono = true,
  className,
}: PillProps) {
  const t = TONES[tone];
  const style: CSSProperties = {
    border: `1px solid ${t.bd}`,
    background: solid ? t.fg : t.bg,
    color: solid ? "var(--bg)" : t.fg,
    fontFamily: mono ? "var(--font-mono)" : "var(--font-body)",
  };
  return (
    <span
      className={[
        "inline-flex items-center gap-1 whitespace-nowrap",
        "px-[6px] py-[1px]",
        "text-[length:var(--font-size-xs)] leading-[1.4]",
        className,
      ]
        .filter(Boolean)
        .join(" ")}
      style={style}
    >
      {children}
    </span>
  );
}
