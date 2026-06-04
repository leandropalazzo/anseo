"use client";

import type { ReactNode } from "react";

export interface TabItem<V extends string = string> {
  value: V;
  label: ReactNode;
  /** Optional numeric/text count rendered after the label. */
  count?: number | string;
}

export interface TabsProps<V extends string = string> {
  value: V;
  onChange: (value: V) => void;
  items: ReadonlyArray<TabItem<V>>;
  className?: string;
}

/**
 * Underline-active tabs — the Signal direction's primary in-page
 * navigation. The active tab gets a 2px accent bar flush with the
 * card border, no other chrome.
 */
export function Tabs<V extends string = string>({
  value,
  onChange,
  items,
  className,
}: TabsProps<V>) {
  return (
    <div
      className={[
        "flex items-center border-b border-[color:var(--border)]",
        className,
      ]
        .filter(Boolean)
        .join(" ")}
      role="tablist"
    >
      {items.map((it) => {
        const active = it.value === value;
        return (
          <button
            key={it.value}
            type="button"
            role="tab"
            aria-selected={active}
            onClick={() => onChange(it.value)}
            className={[
              "relative cursor-pointer appearance-none border-0 bg-transparent",
              "px-[12px] py-[8px]",
              "font-[family-name:var(--font-body)] text-[length:var(--font-size-sm)]",
              "uppercase tracking-[var(--ui-tracking)]",
              active
                ? "text-[color:var(--text)]"
                : "text-[color:var(--text-muted)] hover:text-[color:var(--text)]",
            ].join(" ")}
          >
            {it.label}
            {it.count != null && (
              <span className="ml-[6px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
                {it.count}
              </span>
            )}
            {active && (
              <span
                aria-hidden
                className="absolute inset-x-[8px] -bottom-px h-[2px] bg-[color:var(--accent)]"
              />
            )}
          </button>
        );
      })}
    </div>
  );
}
