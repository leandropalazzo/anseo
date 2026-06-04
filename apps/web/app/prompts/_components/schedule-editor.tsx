import { Cloud } from "lucide-react";

import { ICON_DEFAULTS } from "@/lib/icons";
import type { MockPrompt } from "@/lib/mock";

interface FieldProps {
  label: string;
  value: string;
  mono?: boolean;
}

function Field({ label, value, mono = false }: FieldProps) {
  return (
    <div>
      <div className="label-eyebrow text-[color:var(--text-faint)]">{label}</div>
      <div
        className="mt-[2px] text-[length:var(--font-size-sm)] text-[color:var(--text)]"
        style={{ fontFamily: mono ? "var(--font-mono)" : "var(--font-body)" }}
      >
        {value}
      </div>
    </div>
  );
}

export interface ScheduleEditorProps {
  prompt: MockPrompt;
}

/** Read-only summary of who owns + when a prompt runs. Synced from YAML. */
export function ScheduleEditor({ prompt }: ScheduleEditorProps) {
  // Static label — the prompt-runs panel doesn't actually subscribe to
  // the scheduler clock yet (Phase 2). Fixed value keeps the render pure
  // and avoids a hydration mismatch between server + client.
  const nextLabel = "in 47m · 14:30 UTC";
  return (
    <div className="flex flex-col gap-[12px]">
      <div className="grid grid-cols-2 gap-[10px]">
        <Field label="Cadence" value={prompt.schedule} />
        <Field label="Next run" mono value={nextLabel} />
        <Field label="Owner" mono value="datascience@team" />
        <Field label="Retention" value="90d" />
      </div>
      <div className="flex items-center gap-[8px] border border-[color:var(--border)] bg-[color:var(--bg-sunken)] p-[10px]">
        <Cloud
          size={12}
          strokeWidth={ICON_DEFAULTS.strokeWidth}
          color="var(--text-muted)"
        />
        <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-muted)]">
          synced from prompts.yaml · commit 4f2c9e1 · 12m ago
        </span>
      </div>
    </div>
  );
}
