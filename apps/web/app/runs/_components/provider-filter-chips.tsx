"use client";

import { ProviderDot } from "@/components/ui/provider-dot";
import { resolveProviderIdentity } from "@/lib/provider-colors";

export interface ProviderFilterChipsProps {
  /** null = no filter (all providers selected). Multi-select stored as Set.
   *  Typed as a string set so future provider identities can flow through
   *  without changing the filter state shape. */
  selected: ReadonlySet<string>;
  providers: ReadonlyArray<string>;
  onToggle: (provider: string | null) => void;
}

export function ProviderFilterChips({
  selected,
  providers,
  onToggle,
}: ProviderFilterChipsProps) {
  const allActive = selected.size === 0;
  return (
    <div className="flex items-center gap-[6px] border-b border-[color:var(--hairline)] px-[14px] py-[8px]">
      <span className="mr-[4px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
        provider:
      </span>
      <Chip active={allActive} onClick={() => onToggle(null)}>
        all
      </Chip>
      {providers.map((p) => {
        const label = resolveProviderIdentity(p).label;
        const active = selected.has(p);
        return (
          <Chip key={p} active={active} onClick={() => onToggle(p)}>
            <ProviderDot provider={p} size={6} />
            <span>{label}</span>
          </Chip>
        );
      })}
    </div>
  );
}

function Chip({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="inline-flex cursor-pointer items-center gap-[4px] border border-[color:var(--border)] px-[6px] py-[2px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)]"
      style={{
        background: active ? "var(--bg-elev-2)" : "transparent",
        color: active ? "var(--text)" : "var(--text-muted)",
      }}
      aria-pressed={active}
    >
      {children}
    </button>
  );
}
