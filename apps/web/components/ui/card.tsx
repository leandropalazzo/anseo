import type { CSSProperties, ReactNode } from "react";

export interface CardProps {
  title?: ReactNode;
  eyebrow?: ReactNode;
  action?: ReactNode;
  children?: ReactNode;
  /** When false, render children flush (no inner padding). */
  padding?: boolean;
  /** Adds a 2px accent rail on the left edge. */
  accent?: boolean;
  className?: string;
  style?: CSSProperties;
}

/**
 * Card surface — 1px border on `--bg-elev`, zero-radius in Signal
 * direction (drives the brutalist aesthetic). Header is hidden when
 * no title / eyebrow / action is supplied.
 */
export function Card({
  title,
  eyebrow,
  action,
  children,
  padding = true,
  accent = false,
  className,
  style,
}: CardProps) {
  const hasHeader = Boolean(title || eyebrow || action);
  return (
    <section
      className={[
        "relative overflow-hidden border bg-[color:var(--bg-elev)]",
        "border-[color:var(--border)]",
        className,
      ]
        .filter(Boolean)
        .join(" ")}
      style={style}
    >
      {accent && (
        <div
          aria-hidden
          className="absolute inset-y-0 left-0 w-[2px] bg-[color:var(--accent)]"
        />
      )}
      {hasHeader && (
        <header className="flex items-center justify-between gap-[10px] border-b border-[color:var(--hairline)] px-[14px] py-[10px]">
          <div className="flex min-w-0 flex-col gap-[3px]">
            {eyebrow && (
              <div className="label-eyebrow text-[color:var(--text-faint)]">
                {eyebrow}
              </div>
            )}
            {title && (
              <div className="text-[length:var(--font-size-base)] font-medium leading-tight text-[color:var(--text)]">
                {title}
              </div>
            )}
          </div>
          {action && (
            <div className="flex flex-wrap items-center justify-end gap-[6px]">
              {action}
            </div>
          )}
        </header>
      )}
      <div className={padding ? "p-[14px]" : ""}>{children}</div>
    </section>
  );
}
