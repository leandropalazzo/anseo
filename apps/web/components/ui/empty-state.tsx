import type { ReactNode } from "react";
import { Box } from "lucide-react";
import type { LucideIcon } from "lucide-react";

import { ICON_DEFAULTS } from "@/lib/icons";

export interface EmptyStateProps {
  icon?: LucideIcon;
  title: ReactNode;
  hint?: ReactNode;
  action?: ReactNode;
  className?: string;
}

/** Dashed-border zero-state pane, shown when a list/table has no rows. */
export function EmptyState({
  icon: IconCmp = Box,
  title,
  hint,
  action,
  className,
}: EmptyStateProps) {
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
      <div className="inline-flex border border-[color:var(--border)] bg-[color:var(--bg-elev)] p-[10px] text-[color:var(--text-faint)]">
        <IconCmp size={18} strokeWidth={ICON_DEFAULTS.strokeWidth} />
      </div>
      <div className="mt-[12px] text-[length:var(--font-size-base)] text-[color:var(--text)]">
        {title}
      </div>
      {hint && (
        <div className="mt-[4px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-muted)]">
          {hint}
        </div>
      )}
      {action && <div className="mt-[14px]">{action}</div>}
    </div>
  );
}
