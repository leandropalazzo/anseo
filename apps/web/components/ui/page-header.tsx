import type { ReactNode } from "react";

export interface PageHeaderProps {
  /** Primary page title — rendered as an accessible h1. */
  title: ReactNode;
  /** Optional supporting description line beneath the title. */
  description?: ReactNode;
  /** Optional slot for right-aligned actions (buttons, toggles, etc.). */
  actions?: ReactNode;
  className?: string;
}

/**
 * Shared page-header primitive.
 *
 * Renders a consistent `h1` using the Signal display tokens used across every
 * top-level page section. Replaces the ~18 hand-rolled header blocks that
 * previously lived inline in individual page files.
 *
 * Usage:
 *   <PageHeader title="Visibility" description="Per-prompt trend..." />
 *   <PageHeader title="Alerts" actions={<GenerateButton />} />
 */
export function PageHeader({ title, description, actions, className }: PageHeaderProps) {
  const hasActions = actions != null;

  return (
    <header
      className={[
        hasActions ? "flex items-baseline justify-between gap-[12px]" : undefined,
        className,
      ]
        .filter(Boolean)
        .join(" ")}
    >
      <div>
        <h1 className="m-0 text-[length:22px] font-normal tracking-[var(--display-tracking)] text-[color:var(--text)]">
          {title}
        </h1>
        {description && (
          <p className="m-0 mt-[2px] text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
            {description}
          </p>
        )}
      </div>
      {actions}
    </header>
  );
}
