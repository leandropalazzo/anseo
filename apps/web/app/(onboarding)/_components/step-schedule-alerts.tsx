"use client";

import { useState } from "react";
import { ArrowRight } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { SegControl } from "@/components/ui/seg-control";
import { ICON_DEFAULTS } from "@/lib/icons";
import { DEFAULT_ALERT_TOGGLES } from "@/lib/mock-ops";

type Cadence = "1h" | "6h" | "12h" | "daily" | "weekly";

const CADENCES: ReadonlyArray<{ value: Cadence; label: string }> = [
  { value: "1h", label: "1h" },
  { value: "6h", label: "6h" },
  { value: "12h", label: "12h" },
  { value: "daily", label: "daily" },
  { value: "weekly", label: "weekly" },
];

interface ToggleProps {
  on: boolean;
  ariaLabel: string;
}

function Toggle({ on, ariaLabel }: ToggleProps) {
  const [v, setV] = useState(on);
  return (
    <button
      type="button"
      aria-pressed={v}
      aria-label={ariaLabel}
      onClick={() => setV((p) => !p)}
      className="relative h-[18px] w-[32px] cursor-pointer appearance-none border border-[color:var(--border)]"
      style={{
        background: v ? "var(--accent)" : "var(--bg-sunken)",
        padding: 2,
        borderRadius: 999,
      }}
    >
      <span
        className="absolute top-[2px] h-[12px] w-[12px]"
        style={{
          left: v ? 16 : 2,
          background: v ? "var(--accent-ink)" : "var(--text-muted)",
          borderRadius: 999,
          transition: "left 0.15s",
        }}
      />
    </button>
  );
}

export interface StepScheduleAlertsProps {
  onComplete: () => void;
}

export function StepScheduleAlerts({ onComplete }: StepScheduleAlertsProps) {
  const [cadence, setCadence] = useState<Cadence>("6h");
  return (
    <Card eyebrow="step 5 · schedule & alerts" title="Stay in the loop">
      <div className="grid grid-cols-2 gap-[14px]">
        <div>
          <div className="label-eyebrow text-[color:var(--text-faint)]">
            cadence
          </div>
          <div className="mt-[6px]">
            <SegControl<Cadence>
              value={cadence}
              onChange={setCadence}
              options={CADENCES}
              ariaLabel="cadence"
            />
          </div>
        </div>
        <div>
          <div className="label-eyebrow text-[color:var(--text-faint)]">
            alerts
          </div>
          <div className="mt-[6px] flex flex-col gap-[6px]">
            {DEFAULT_ALERT_TOGGLES.map((x) => (
              <label
                key={x.label}
                className="flex items-center gap-[6px] text-[length:var(--font-size-sm)] text-[color:var(--text)]"
              >
                <Toggle on={x.on} ariaLabel={x.label} /> {x.label}
              </label>
            ))}
          </div>
        </div>
      </div>
      <div className="mt-[16px] flex justify-end gap-[8px]">
        <Button variant="ghost" size="sm" onClick={onComplete}>
          Skip
        </Button>
        <Button
          variant="primary"
          size="sm"
          onClick={onComplete}
          leadingIcon={
            <ArrowRight size={11} strokeWidth={ICON_DEFAULTS.strokeWidth} />
          }
          data-testid="onboarding-complete"
        >
          Open dashboard
        </Button>
      </div>
    </Card>
  );
}
