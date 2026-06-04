"use client";

import type { ReactNode } from "react";

export interface SegControlOption<V extends string = string> {
  value: V;
  label: ReactNode;
  icon?: ReactNode;
}

export interface SegControlProps<V extends string = string> {
  value: V;
  onChange: (value: V) => void;
  options: ReadonlyArray<SegControlOption<V>>;
  ariaLabel?: string;
  className?: string;
}

/**
 * Segmented control — sunken-track surface with an elevated active
 * segment. Used for binary/ternary mode toggles (deployment switch,
 * timescale selectors). Matches `DeploymentSwitch` in the prototype.
 */
export function SegControl<V extends string = string>({
  value,
  onChange,
  options,
  ariaLabel,
  className,
}: SegControlProps<V>) {
  return (
    <div
      role="radiogroup"
      aria-label={ariaLabel}
      className={[
        "inline-flex border border-[color:var(--border)] bg-[color:var(--bg-sunken)] p-[2px]",
        className,
      ]
        .filter(Boolean)
        .join(" ")}
    >
      {options.map((o) => {
        const active = o.value === value;
        return (
          <button
            key={o.value}
            type="button"
            role="radio"
            aria-checked={active}
            onClick={() => onChange(o.value)}
            className={[
              "inline-flex cursor-pointer items-center gap-[5px] border-0",
              "px-[8px] py-[3px]",
              "font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)]",
              active
                ? "bg-[color:var(--bg-elev)] text-[color:var(--text)]"
                : "bg-transparent text-[color:var(--text-muted)] hover:text-[color:var(--text)]",
            ].join(" ")}
          >
            {o.icon}
            {o.label}
          </button>
        );
      })}
    </div>
  );
}
