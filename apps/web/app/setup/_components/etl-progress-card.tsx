"use client";

import { useRouter } from "next/navigation";
import { useState } from "react";
import { Card } from "@/components/ui/card";
import { Pill } from "@/components/ui/pill";
import { Button } from "@/components/ui/button";
import type { PillTone } from "@/components/ui/pill";
import type { ClickHouseEtlStatus } from "@/lib/api";
import { postClickHouseEtlResume } from "@/lib/api";

interface Props {
  etl: ClickHouseEtlStatus;
}

function stateTone(state: ClickHouseEtlStatus["state"]): PillTone {
  if (state === "completed") return "ok";
  if (state === "running") return "info";
  if (state === "interrupted") return "warn";
  if (state === "idle") return "neutral";
  return "danger";
}

function progressPercent(done: number | null, total: number | null): number {
  if (done === null || total === null || total === 0) return 0;
  return Math.min(100, Math.round((done / total) * 100));
}

export function EtlProgressCard({ etl }: Props) {
  const router = useRouter();
  const [resuming, setResuming] = useState(false);

  const tone = stateTone(etl.state);
  const pct = progressPercent(etl.batches_done, etl.batches_total);
  const showProgress =
    etl.state === "running" ||
    etl.state === "interrupted" ||
    etl.state === "completed";

  async function handleResume() {
    setResuming(true);
    try {
      await postClickHouseEtlResume();
      router.refresh();
    } finally {
      setResuming(false);
    }
  }

  return (
    <div data-testid="etl-progress-card">
      <Card
        eyebrow="etl migration"
        title="ClickHouse ETL"
        action={
          <span data-testid="etl-state-badge">
            <Pill tone={tone}>
              <span
                aria-hidden
                className="mr-[4px] inline-block h-[6px] w-[6px] rounded-full"
                style={{
                  background:
                    tone === "ok"
                      ? "var(--ok)"
                      : tone === "info"
                        ? "var(--info)"
                        : tone === "warn"
                          ? "var(--warn)"
                          : tone === "danger"
                            ? "var(--danger)"
                            : "var(--text-faint)",
                }}
              />
              {etl.state}
            </Pill>
          </span>
        }
      >
        <div className="flex flex-col gap-[10px]">
          {etl.state === "idle" && (
            <p className="m-0 text-[length:var(--font-size-sm)] text-[color:var(--text-faint)]">
              Not started — install ClickHouse to begin ETL
            </p>
          )}

          {etl.state === "unknown" && (
            <p className="m-0 text-[length:var(--font-size-sm)] text-[color:var(--text-faint)]">
              Status unavailable
            </p>
          )}

          {etl.state === "running" && (
            <p className="m-0 text-[length:var(--font-size-sm)] text-[color:var(--text-faint)]">
              In progress&hellip;
            </p>
          )}

          {etl.state === "completed" && (
            <Pill tone="ok" mono={false}>
              Migration complete
            </Pill>
          )}

          {showProgress && (
            <>
              <div
                className="flex items-center justify-between gap-[8px]"
                data-testid="etl-progress"
              >
                <span className="text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
                  Batches
                </span>
                <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text)]">
                  {etl.batches_done !== null ? etl.batches_done : "—"}
                  {" / "}
                  {etl.batches_total !== null ? etl.batches_total : "—"}
                </span>
              </div>

              <div
                className="h-[6px] w-full overflow-hidden"
                style={{ background: "var(--hairline)" }}
                role="progressbar"
                aria-label="ETL backfill progress"
                aria-valuenow={pct}
                aria-valuemin={0}
                aria-valuemax={100}
              >
                <div
                  className="h-full transition-[width] duration-300"
                  style={{
                    width: `${pct}%`,
                    background:
                      etl.state === "completed"
                        ? "var(--ok)"
                        : etl.state === "interrupted"
                          ? "var(--warn)"
                          : "var(--info)",
                  }}
                />
              </div>
            </>
          )}

          {etl.state === "interrupted" && (
            <div className="mt-[4px] flex items-center gap-[8px]">
              <Button
                variant="secondary"
                size="sm"
                disabled={resuming}
                onClick={() => void handleResume()}
                data-testid="etl-resume-button"
              >
                {resuming ? "Resuming…" : "Resume"}
              </Button>
              <span className="text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
                ETL was interrupted — click to continue
              </span>
            </div>
          )}

          {etl.error && (
            <p className="m-0 text-[length:var(--font-size-xs)] text-[color:var(--danger)]">
              {etl.error}
            </p>
          )}
        </div>
      </Card>
    </div>
  );
}
