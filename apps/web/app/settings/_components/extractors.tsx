"use client";

import { useState } from "react";

import { Card } from "@/components/ui/card";
import { EXTRACTORS } from "@/lib/mock-ops";

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

export function ExtractorsSection() {
  return (
    <Card eyebrow="how we parse provider responses" title="Extractors">
      <div className="flex flex-col gap-[10px]">
        {EXTRACTORS.map((x) => (
          <div
            key={x.name}
            className="grid items-center gap-[10px] border-b border-[color:var(--hairline)] py-[8px] [grid-template-columns:200px_1fr_80px]"
          >
            <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-sm)] text-[color:var(--text)]">
              {x.name}
            </span>
            <span className="text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
              {x.detail}
            </span>
            <Toggle on={x.enabled} ariaLabel={`toggle ${x.name}`} />
          </div>
        ))}
      </div>
    </Card>
  );
}
