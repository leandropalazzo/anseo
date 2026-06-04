"use client";

import { ArrowRight, Check, Cloud, Server } from "lucide-react";
import type { LucideIcon } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { CodeBlock } from "@/components/ui/code-block";
import { ICON_DEFAULTS } from "@/lib/icons";

interface DeployCardProps {
  active: boolean;
  icon: LucideIcon;
  title: string;
  sub: string;
  bullets: ReadonlyArray<string>;
}

function DeployCard({ active, icon: Icon, title, sub, bullets }: DeployCardProps) {
  return (
    <div
      className="relative cursor-pointer p-[14px]"
      style={{
        border: `1px solid ${active ? "var(--accent)" : "var(--border)"}`,
        background: active
          ? "color-mix(in oklch, var(--accent) 6%, var(--bg-elev))"
          : "var(--bg-elev)",
      }}
    >
      {active && (
        <span className="absolute right-[10px] top-[10px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--accent)]">
          ● selected
        </span>
      )}
      <div className="flex items-center gap-[8px]">
        <Icon
          size={16}
          strokeWidth={ICON_DEFAULTS.strokeWidth}
          color={active ? "var(--accent)" : "var(--text-muted)"}
        />
        <span className="text-[length:18px] text-[color:var(--text)]">
          {title}
        </span>
      </div>
      <p className="m-0 mb-[10px] mt-[6px] text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
        {sub}
      </p>
      <ul className="m-0 flex flex-col gap-[4px] list-none p-0">
        {bullets.map((b, i) => (
          <li
            key={i}
            className="grid grid-cols-[16px_1fr] items-center gap-[6px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-muted)]"
          >
            <Check
              size={11}
              strokeWidth={ICON_DEFAULTS.strokeWidth}
              color="var(--ok)"
            />
            {b}
          </li>
        ))}
      </ul>
    </div>
  );
}

export interface StepInitProps {
  onNext: () => void;
}

export function StepInit({ onNext }: StepInitProps) {
  return (
    <Card eyebrow="step 1 · init" title="Choose your deployment">
      <div className="grid grid-cols-2 gap-[12px]">
        <DeployCard
          active
          icon={Server}
          title="Local"
          sub="Docker Compose · zero telemetry · keys in your keychain"
          bullets={[
            "Self-hosted, MIT-licensed",
            "Postgres + worker + api + web",
            "All data stays on your machine",
          ]}
        />
        <DeployCard
          active={false}
          icon={Cloud}
          title="Cloud"
          sub="Managed · SSO · audit log · team sharing"
          bullets={[
            "Multi-region (US/EU/APAC)",
            "Team & RBAC included",
            "Pay only for what you run",
          ]}
        />
      </div>
      <div className="mt-[12px]">
        <div className="mb-[6px] label-eyebrow text-[color:var(--text-faint)]">
          or run from CLI:
        </div>
        <CodeBlock lang="bash" code={"ogeo init --local\nogeo compose up"} />
      </div>
      <div className="mt-[16px] flex justify-end">
        <Button
          variant="primary"
          size="sm"
          onClick={onNext}
          leadingIcon={
            <ArrowRight size={11} strokeWidth={ICON_DEFAULTS.strokeWidth} />
          }
        >
          Continue
        </Button>
      </div>
    </Card>
  );
}
