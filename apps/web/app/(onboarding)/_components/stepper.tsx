"use client";

import { Check } from "lucide-react";
import type { LucideIcon } from "lucide-react";

import { ICON_DEFAULTS } from "@/lib/icons";

export interface StepperStep {
  id: string;
  label: string;
  icon: LucideIcon;
}

export interface StepperProps {
  steps: ReadonlyArray<StepperStep>;
  active: number;
  onSelect: (index: number) => void;
}

/** Vertical step rail with hairline connector + accent fill on done steps. */
export function Stepper({ steps, active, onSelect }: StepperProps) {
  return (
    <div className="relative flex flex-col">
      <div
        aria-hidden
        className="absolute top-[24px] bottom-[24px] left-[12px] w-px bg-[color:var(--border)]"
      />
      {steps.map((s, i) => {
        const done = i < active;
        const isActive = i === active;
        const Icon = s.icon;
        return (
          <button
            key={s.id}
            type="button"
            onClick={() => onSelect(i)}
            className="grid cursor-pointer appearance-none grid-cols-[26px_1fr] items-center gap-[10px] border-0 bg-transparent py-[8px] text-left"
            data-testid={`step-nav-${s.id}`}
          >
            <span
              className="relative z-[1] inline-flex h-[24px] w-[24px] items-center justify-center"
              style={{
                background: done
                  ? "var(--accent)"
                  : isActive
                    ? "var(--bg-elev)"
                    : "var(--bg-sunken)",
                border: `1px solid ${done || isActive ? "var(--accent)" : "var(--border)"}`,
                color: done
                  ? "var(--accent-ink)"
                  : isActive
                    ? "var(--accent)"
                    : "var(--text-faint)",
              }}
            >
              {done ? (
                <Check
                  size={12}
                  strokeWidth={ICON_DEFAULTS.strokeWidth}
                  color="var(--accent-ink)"
                />
              ) : (
                <Icon size={12} strokeWidth={ICON_DEFAULTS.strokeWidth} />
              )}
            </span>
            <span>
              <div
                className="text-[length:var(--font-size-sm)]"
                style={{
                  color: isActive ? "var(--text)" : "var(--text-muted)",
                  fontWeight: isActive ? 500 : 400,
                }}
              >
                {s.label}
              </div>
              <div className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
                step {i + 1} / {steps.length}
              </div>
            </span>
          </button>
        );
      })}
    </div>
  );
}
