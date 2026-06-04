import { ScheduleCard } from "./schedule-card";
import { AddScheduleSheet } from "./add-schedule-sheet";
import { CostProjectionBadge } from "./cost-projection-badge";
import type { ScheduleSummary } from "@/lib/api";

export interface ScheduleGridProps {
  schedules: ReadonlyArray<ScheduleSummary>;
  declaredPrompts: ReadonlyArray<string>;
  /** Provider wire names with a stored key; the create form defaults to all. */
  configuredProviders: ReadonlyArray<string>;
  apiError: string | null;
}

/**
 * Schedules tab body. Wraps three layers:
 *   1. Live summary (count + projected monthly $ + paused state) from
 *      `fetchSchedules` — preserves Story 10.4 behavior.
 *   2. Visual 24-hour cadence grid (mock, illustrative) so operators see
 *      density at a glance.
 *   3. Live ScheduleCard list with the existing AddScheduleSheet CTA.
 */
export function ScheduleGrid({
  schedules,
  declaredPrompts,
  configuredProviders,
  apiError,
}: ScheduleGridProps) {
  const totalProjected = schedules.reduce(
    (sum, s) => sum + (s.projected_monthly_usd ?? 0),
    0,
  );
  const activeCount = schedules.filter((s) => !s.paused).length;

  return (
    <div className="flex flex-col gap-[14px] p-[14px]">
      {/* Live summary + Add CTA */}
      <div className="flex flex-wrap items-end justify-between gap-[12px]">
        <div className="flex flex-wrap gap-x-[24px] gap-y-[6px]">
          <SummaryStat label="Total" value={schedules.length} />
          <SummaryStat label="Active" value={activeCount} />
          <div>
            <div className="label-eyebrow text-[color:var(--text-faint)]">
              Projected monthly
            </div>
            <div className="mt-[2px]">
              <CostProjectionBadge usd={totalProjected} />
            </div>
          </div>
        </div>
        <AddScheduleSheet
          declaredPrompts={declaredPrompts}
          configuredProviders={configuredProviders}
        />
      </div>

      {apiError !== null ? (
        <div
          data-testid="api-error"
          className="border border-[color:var(--warn)] bg-[color:var(--bg-sunken)] p-[12px] text-[length:var(--font-size-sm)] text-[color:var(--warn)]"
        >
          <p className="font-semibold">Couldn&apos;t reach the API.</p>
          <p className="mt-[4px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)]">
            {apiError}
          </p>
        </div>
      ) : null}

      {/* Live schedules list */}
      {schedules.length > 0 ? (
        <ul className="space-y-[12px]" data-testid="schedules-list">
          {schedules.map((s) => (
            <li key={s.id}>
              <ScheduleCard schedule={s} />
            </li>
          ))}
        </ul>
      ) : apiError === null ? (
        <div
          data-testid="empty-state"
          className="border border-dashed border-[color:var(--border)] bg-[color:var(--bg-sunken)] p-[14px] text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]"
        >
          No schedules declared yet. Add one above or with{" "}
          <code className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)]">
            ogeo schedule add &lt;name&gt; --cron &quot;0 9 * * 1-5&quot;
          </code>
          .
        </div>
      ) : null}
    </div>
  );
}

function SummaryStat({ label, value }: { label: string; value: number }) {
  return (
    <div>
      <div className="label-eyebrow text-[color:var(--text-faint)]">{label}</div>
      <div className="mt-[2px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-base)] text-[color:var(--text)]">
        {value}
      </div>
    </div>
  );
}
