"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { FileCode, Plus } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Pill } from "@/components/ui/pill";
import { EmptyState } from "@/components/ui/empty-state";
import { ICON_DEFAULTS } from "@/lib/icons";
import {
  setAlertRuleStatus,
  type AlertRule,
  type AlertRuleStatus,
} from "@/lib/api/alerts";

export interface AlertRulesProps {
  rules: ReadonlyArray<AlertRule>;
}

const HDR_CLASS =
  "border-b border-[color:var(--border)] px-[12px] py-[6px] text-left label-eyebrow text-[color:var(--text-faint)] font-medium";
const TD_CLASS =
  "px-[12px] py-[6px] text-[length:var(--font-size-sm)] text-[color:var(--text)] whitespace-nowrap";

export function AlertRules({ rules }: AlertRulesProps) {
  const router = useRouter();
  // Name of the rule whose status toggle is in flight.
  const [pending, setPending] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function toggle(name: string, current: AlertRuleStatus) {
    const next: AlertRuleStatus = current === "armed" ? "muted" : "armed";
    setPending(name);
    setError(null);
    try {
      await setAlertRuleStatus(name, next);
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setPending(null);
    }
  }

  return (
    <div>
      <div className="flex items-center justify-between border-b border-[color:var(--hairline)] px-[14px] py-[10px]">
        <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
          defined in alerts.yaml
        </span>
        <div className="flex gap-[6px]">
          <Button
            size="sm"
            variant="ghost"
            leadingIcon={
              <FileCode size={11} strokeWidth={ICON_DEFAULTS.strokeWidth} />
            }
          >
            Edit YAML
          </Button>
          <Button
            size="sm"
            variant="primary"
            leadingIcon={
              <Plus size={11} strokeWidth={ICON_DEFAULTS.strokeWidth} />
            }
          >
            New rule
          </Button>
        </div>
      </div>

      {error && (
        <div
          data-testid="rule-action-error"
          role="alert"
          className="border-b border-[color:var(--hairline)] px-[14px] py-[8px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--danger)]"
        >
          {error}
        </div>
      )}

      {rules.length === 0 ? (
        <div className="p-[14px]">
          <EmptyState
            icon={FileCode}
            title="No alert rules yet."
            hint="Define rules in alerts.yaml to be notified when anomalies fire."
          />
        </div>
      ) : (
        <table className="w-full border-collapse">
          <thead>
            <tr style={{ background: "var(--bg-sunken)" }}>
              <th className={HDR_CLASS}>rule</th>
              <th className={HDR_CLASS}>condition</th>
              <th className={HDR_CLASS}>target</th>
              <th className={HDR_CLASS}>channels</th>
              <th className={HDR_CLASS}>fires (7d)</th>
              <th className={HDR_CLASS}>state</th>
              <th className={HDR_CLASS}></th>
            </tr>
          </thead>
          <tbody>
            {rules.map((r) => (
              <tr
                key={r.name}
                className="border-b border-[color:var(--hairline)]"
                data-testid={`rule-${r.name}`}
              >
                <td className={TD_CLASS}>
                  <span className="font-[family-name:var(--font-mono)] text-[color:var(--text)]">
                    {r.name}
                  </span>
                </td>
                <td className={TD_CLASS}>
                  <code className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-muted)]">
                    {r.on}
                  </code>
                </td>
                <td className={TD_CLASS}>
                  <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)]">
                    {r.target}
                  </span>
                </td>
                <td className={TD_CLASS}>
                  <div className="flex gap-[4px]">
                    {r.channels.map((c) => (
                      <Pill key={c} mono>
                        {c}
                      </Pill>
                    ))}
                  </div>
                </td>
                <td className={TD_CLASS}>
                  <span
                    className="font-[family-name:var(--font-mono)]"
                    style={{
                      color: r.fires > 0 ? "var(--warn)" : "var(--text-muted)",
                    }}
                  >
                    {r.fires}
                  </span>
                </td>
                <td className={TD_CLASS}>
                  <Pill mono tone={r.status === "armed" ? "ok" : "neutral"}>
                    <span
                      className="mr-[4px] inline-block h-[6px] w-[6px] rounded-full"
                      style={{
                        background:
                          r.status === "armed"
                            ? "var(--ok)"
                            : "var(--text-faint)",
                      }}
                    />
                    {r.status}
                  </Pill>
                </td>
                <td className={TD_CLASS}>
                  <Button
                    size="sm"
                    variant="ghost"
                    disabled={pending !== null}
                    data-testid={`rule-toggle-${r.name}`}
                    onClick={() => toggle(r.name, r.status)}
                  >
                    {r.status === "armed" ? "Mute" : "Arm"}
                  </Button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
