import { Card } from "@/components/ui/card";
import { Pill } from "@/components/ui/pill";
import type { PillTone } from "@/components/ui/pill";
import type { SetupStatus } from "@/lib/api";

type PostgresState = SetupStatus["postgres"]["state"];

function stateTone(state: PostgresState): PillTone {
  if (state === "healthy") return "ok";
  if (state === "degraded") return "warn";
  return "danger";
}

interface Props {
  postgres: SetupStatus["postgres"];
}

export function PostgresCard({ postgres }: Props) {
  const tone = stateTone(postgres.state);
  return (
    <Card
      eyebrow="database"
      title="Postgres"
      action={
        <Pill tone={tone}>
          <span
            aria-hidden
            className="mr-[4px] inline-block h-[6px] w-[6px] rounded-full"
            style={{ background: tone === "ok" ? "var(--ok)" : tone === "warn" ? "var(--warn)" : "var(--danger)" }}
          />
          {postgres.state}
        </Pill>
      }
    >
      <div className="flex flex-col gap-[8px]">
        <StatusRow label="Schema Version" value={postgres.schema_version !== null ? String(postgres.schema_version) : "—"} />
        <StatusRow label="Row Count Estimate" value={postgres.row_count_estimate !== null ? postgres.row_count_estimate.toLocaleString() : "—"} />
        <StatusRow label="Last Write" value={postgres.last_write_at !== null ? new Date(postgres.last_write_at).toLocaleString() : "—"} />
        {postgres.error && (
          <p className="mt-[4px] m-0 text-[length:var(--font-size-xs)] text-[color:var(--danger)]">
            {postgres.error}
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
