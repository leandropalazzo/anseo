import { Pill } from "@/components/ui/pill";
import type { CapabilityUsage } from "@/lib/dev-mode";

// UX-DR123 — the capability inspector shows a declared-vs-used diff so a plugin
// author can spot both unused grants (over-declaration) and, more seriously,
// undeclared capabilities that were exercised at runtime (a policy violation).

export type CapabilityDiffState = "ok" | "unused" | "undeclared";

export function diffState(c: CapabilityUsage): CapabilityDiffState {
  if (c.used && !c.declared) return "undeclared";
  if (c.declared && !c.used) return "unused";
  return "ok";
}

const STATE_TONE = {
  ok: "ok",
  unused: "warn",
  undeclared: "danger",
} as const;

const STATE_LABEL = {
  ok: "declared + used",
  unused: "declared, unused",
  undeclared: "UNDECLARED USE",
} as const;

export function CapabilityInspector({
  capabilities,
}: {
  capabilities: CapabilityUsage[];
}) {
  return (
    <div
      data-testid="capability-inspector"
      className="flex flex-col gap-[6px]"
    >
      <div className="label-eyebrow text-[color:var(--text-faint)]">
        declared vs. used
      </div>
      <ul className="m-0 flex list-none flex-col gap-[4px] p-0">
        {capabilities.map((c) => {
          const state = diffState(c);
          return (
            <li
              key={c.capability}
              data-testid="capability-row"
              data-capability={c.capability}
              data-diff={state}
              className="flex items-center justify-between gap-[8px] border border-[color:var(--border)] px-[8px] py-[4px]"
            >
              <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-muted)]">
                {c.capability}
              </span>
              <Pill mono tone={STATE_TONE[state]}>
                {STATE_LABEL[state]}
              </Pill>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
