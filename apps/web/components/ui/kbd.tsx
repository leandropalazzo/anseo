import type { ReactNode } from "react";

export interface KbdProps {
  children: ReactNode;
  className?: string;
}

/** Keyboard chip — single character or short combo (e.g. `⌘K`, `esc`). */
export function KBD({ children, className }: KbdProps) {
  return (
    <kbd
      className={[
        "inline-flex items-center justify-center",
        "min-w-4 px-[5px] py-[1px]",
        "border border-[color:var(--border)] bg-[color:var(--bg-elev-2)]",
        "font-[family-name:var(--font-mono)]",
        "text-[length:var(--font-size-xs)] leading-[1.4]",
        "text-[color:var(--text-muted)]",
        className,
      ]
        .filter(Boolean)
        .join(" ")}
    >
      {children}
    </kbd>
  );
}
