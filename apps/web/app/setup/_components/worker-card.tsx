import { Card } from "@/components/ui/card";
import { Pill } from "@/components/ui/pill";
import type { PillTone } from "@/components/ui/pill";
import type { SetupStatus } from "@/lib/api";

type WorkerState = SetupStatus["worker"]["state"];

function stateTone(state: WorkerState): PillTone {
  if (state === "running") return "ok";
  return "danger";
}

interface Props {
  worker: SetupStatus["worker"];
}

export function WorkerCard({ worker }: Props) {
  const tone = stateTone(worker.state);
  return (
    <Card
      eyebrow="background process"
      title="Worker"
      action={
        <Pill tone={tone}>
          <span
            aria-hidden
            className="mr-[4px] inline-block h-[6px] w-[6px] rounded-full"
            style={{ background: tone === "ok" ? "var(--ok)" : "var(--danger)" }}
          />
          {worker.state}
        </Pill>
      }
    >
      <div className="flex flex-col gap-[8px]">
        <StatusRow label="State" value={worker.state} />
        <StatusRow
          label="Queue Depth"
          value={worker.queue_depth !== null ? String(worker.queue_depth) : "—"}
        />
        {worker.uptime_seconds !== null && (
          <StatusRow label="Uptime" value={`${worker.uptime_seconds}s`} />
        )}
        {worker.error && (
          <p className="mt-[4px] m-0 text-[length:var(--font-size-xs)] text-[color:var(--danger)]">
            {worker.error}
          </p>
        )}
      </div>
    </Card>
  );
}

function StatusRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between gap-[8px]">
      <span className="text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">{label}</span>
      <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text)]">
        {value}
      </span>
    </div>
  );
}
