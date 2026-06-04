import type { CSSProperties } from "react";

export type DiffLineType = "add" | "rem" | "ctx";

const TONES: Readonly<Record<DiffLineType, { bg: string; fg: string; mark: string }>> = {
  add: { bg: "color-mix(in oklch, var(--ok) 14%, transparent)",     fg: "var(--ok)",     mark: "+" },
  rem: { bg: "color-mix(in oklch, var(--danger) 14%, transparent)", fg: "var(--danger)", mark: "−" },
  ctx: { bg: "transparent",                                          fg: "var(--text-muted)", mark: " " },
};

export interface DiffLineProps {
  type: DiffLineType;
  text: string;
}

/** Single line of a unified YAML diff (additions, removals, context). */
export function DiffLine({ type, text }: DiffLineProps) {
  const t = TONES[type];
  const style: CSSProperties = { background: t.bg };
  return (
    <div
      className="grid grid-cols-[20px_1fr] font-[family-name:var(--font-mono)] text-[length:var(--font-size-sm)] leading-[1.7]"
      style={style}
    >
      <span className="text-center" style={{ color: t.fg }}>
        {t.mark}
      </span>
      <span style={{ color: type === "ctx" ? "var(--text)" : t.fg }}>{text}</span>
    </div>
  );
}
