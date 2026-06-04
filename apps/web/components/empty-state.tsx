import type { ReactNode } from "react";

export interface EmptyStateProps {
  title: ReactNode;
  message: ReactNode;
  className?: string;
}

/**
 * Zero-state pane shown when a live API surface has no rows and the dashboard
 * is NOT in demo mode (see `lib/data-source.ts`). Dashed border on the sunken
 * surface, matching the Signal design tokens used across `app/_components`.
 */
export function EmptyState({ title, message, className }: EmptyStateProps) {
  return (
    <div
      className={[
        "border border-dashed border-[color:var(--border)] bg-[color:var(--bg-sunken)]",
        "px-[24px] py-[32px] text-center",
        className,
      ]
        .filter(Boolean)
        .join(" ")}
    >
      <div className="text-[length:var(--font-size-base)] text-[color:var(--text)]">
        {title}
      </div>
      <div className="mt-[4px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-muted)]">
        {message}
      </div>
    </div>
  );
}
