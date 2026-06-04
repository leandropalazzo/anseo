"use client";

import { useState, useTransition } from "react";
import { useRouter } from "next/navigation";
import type { ScheduleSummary } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Pill } from "@/components/ui/pill";
import { LocalTime } from "@/app/_components/local-time";
import { CostProjectionBadge } from "./cost-projection-badge";

async function mutateSchedule(
  id: string,
  method: "PUT" | "DELETE",
  payload?: unknown,
): Promise<void> {
  const r = await fetch(`/api/schedules/${encodeURIComponent(id)}`, {
    method,
    headers: payload ? { "Content-Type": "application/json" } : undefined,
    body: payload ? JSON.stringify(payload) : undefined,
  });
  if (!r.ok && r.status !== 204) {
    const body = await r.json().catch(() => null);
    const message =
      typeof body?.message === "string"
        ? body.message
        : `${method} /api/schedules/${id} → ${r.status}`;
    throw new Error(message);
  }
}

export function ScheduleCard({ schedule }: { schedule: ScheduleSummary }) {
  const router = useRouter();
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [running, setRunning] = useState(false);
  const [isPending, startTransition] = useTransition();

  // Mutations (pause / delete) change the row's shape, so they soft-refresh the
  // server component once done. They share `isPending` to disable themselves.
  const mutate = (fn: () => Promise<void>) => {
    setError(null);
    setNotice(null);
    startTransition(async () => {
      try {
        await fn();
        router.refresh();
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      }
    });
  };

  // "Run now" dispatches the schedule's prompts in the background. It does NOT
  // lock the rest of the card — the run executes server-side and lands in the
  // run log; we just kick it off and soft-refresh so the new tick shows up.
  const runNow = () => {
    setError(null);
    setNotice(null);
    setRunning(true);
    void (async () => {
      try {
        const r = await fetch(
          `/api/schedules/${encodeURIComponent(schedule.id)}/run`,
          { method: "POST" },
        );
        if (!r.ok) {
          const body = await r.json().catch(() => null);
          const message =
            typeof body?.message === "string"
              ? body.message
              : `POST /api/schedules/${schedule.id}/run → ${r.status}`;
          throw new Error(message);
        }
        setNotice("Run dispatched — results will appear in the run log.");
        router.refresh();
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setRunning(false);
      }
    })();
  };

  const togglePause = () =>
    mutate(() =>
      mutateSchedule(schedule.id, "PUT", { paused: !schedule.paused }),
    );
  const remove = () => {
    if (
      typeof window !== "undefined" &&
      !window.confirm(`Delete schedule "${schedule.name}"? This cannot be undone.`)
    ) {
      return;
    }
    mutate(() => mutateSchedule(schedule.id, "DELETE"));
  };

  return (
    <article
      data-testid={`schedule-card-${schedule.name}`}
      className="space-y-3 border border-[color:var(--border)] bg-[color:var(--bg-elev)] p-4"
    >
      <header className="flex items-start justify-between gap-3">
        <div>
          <h2 className="text-base font-semibold tracking-tight text-[color:var(--text)]">
            {schedule.name}
          </h2>
          <div className="mt-1 font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
            cron{" "}
            <span className="text-[color:var(--text)]">{schedule.cron}</span>
            {" · debounce "}
            {schedule.debounce_minutes}m
          </div>
        </div>
        <div className="flex flex-col items-end gap-1.5">
          <Pill tone={schedule.paused ? "neutral" : "ok"}>
            {schedule.paused ? "Paused" : "Active"}
          </Pill>
          {schedule.projected_monthly_usd !== null ? (
            <CostProjectionBadge
              usd={schedule.projected_monthly_usd}
              acknowledged={schedule.projection_acknowledged_at !== null}
            />
          ) : null}
        </div>
      </header>

      <dl className="grid grid-cols-1 gap-x-6 gap-y-2 text-[length:var(--font-size-sm)] sm:grid-cols-2">
        <div>
          <dt className="label-eyebrow text-[color:var(--text-faint)]">
            Prompts ({schedule.prompts.length})
          </dt>
          <dd className="mt-0.5 font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text)]">
            {schedule.prompts.length === 0 ? (
              <span className="text-[color:var(--text-faint)]">—</span>
            ) : (
              schedule.prompts.join(", ")
            )}
          </dd>
        </div>
        <div>
          <dt className="label-eyebrow text-[color:var(--text-faint)]">
            Providers ({schedule.providers.length})
          </dt>
          <dd className="mt-0.5 font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text)]">
            {schedule.providers.length === 0 ? (
              <span className="text-[color:var(--text-faint)]">—</span>
            ) : (
              schedule.providers.join(", ")
            )}
          </dd>
        </div>
        <div>
          <dt className="label-eyebrow text-[color:var(--text-faint)]">
            Created
          </dt>
          <dd className="mt-0.5 font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-muted)]">
            {schedule.created_at ? (
              <LocalTime iso={schedule.created_at} mode="datetime" />
            ) : (
              "—"
            )}
          </dd>
        </div>
        <div>
          <dt className="label-eyebrow text-[color:var(--text-faint)]">
            Last tick
          </dt>
          <dd className="mt-0.5 font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-muted)]">
            {schedule.last_tick_at ? (
              <LocalTime iso={schedule.last_tick_at} mode="datetime" />
            ) : (
              "—"
            )}
            {schedule.last_tick_status ? (
              <span className="ml-2 text-[color:var(--text-faint)]">
                ({schedule.last_tick_status})
              </span>
            ) : null}
          </dd>
        </div>
      </dl>

      {notice ? (
        <p
          data-testid={`schedule-notice-${schedule.name}`}
          className="border border-[color:color-mix(in_oklch,var(--ok)_40%,transparent)] bg-[color:color-mix(in_oklch,var(--ok)_12%,transparent)] px-2 py-1 text-[length:var(--font-size-xs)] text-[color:var(--ok)]"
          role="status"
        >
          {notice}
        </p>
      ) : null}

      {error ? (
        <p
          data-testid={`schedule-error-${schedule.name}`}
          className="border border-[color:color-mix(in_oklch,var(--danger)_40%,transparent)] bg-[color:color-mix(in_oklch,var(--danger)_12%,transparent)] px-2 py-1 text-[length:var(--font-size-xs)] text-[color:var(--danger)]"
          role="alert"
        >
          {error}
        </p>
      ) : null}

      <div className="flex justify-end gap-2 border-t border-[color:var(--hairline)] pt-3">
        <Button
          variant="primary"
          size="sm"
          data-testid={`run-now-${schedule.name}`}
          onClick={runNow}
          disabled={running}
        >
          {running ? "Dispatching…" : "Run now"}
        </Button>
        <Button
          variant="secondary"
          size="sm"
          data-testid={`toggle-pause-${schedule.name}`}
          onClick={togglePause}
          disabled={isPending}
        >
          {schedule.paused ? "Resume" : "Pause"}
        </Button>
        <Button
          variant="danger"
          size="sm"
          data-testid={`delete-${schedule.name}`}
          onClick={remove}
          disabled={isPending}
        >
          Delete
        </Button>
      </div>
    </article>
  );
}
