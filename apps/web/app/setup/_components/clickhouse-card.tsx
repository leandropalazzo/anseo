"use client";

import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { Card } from "@/components/ui/card";
import { Pill } from "@/components/ui/pill";
import { Button } from "@/components/ui/button";
import type { PillTone } from "@/components/ui/pill";
import type { SetupStatus, InstallEvent, InstallStep } from "@/lib/api";
import { postClickHouseInstall, streamClickHouseInstall } from "@/lib/api";

type ClickHouseState = SetupStatus["clickhouse"]["state"];

function stateTone(state: ClickHouseState): PillTone {
  if (state === "healthy") return "ok";
  if (state === "degraded") return "warn";
  if (state === "not_configured") return "neutral";
  return "danger";
}

/** Operator-facing copy per install step (wire keys → UI labels). */
const STEP_LABELS: Record<string, string> = {
  docker_detected: "Detecting Docker",
  image_pulling: "Pulling image",
  container_starting: "Spawning container",
  provisioning_user: "Provisioning user",
  applying_migrations: "Applying migrations",
  running_parity_test: "Running parity test",
  complete: "Complete",
};

/** Docker availability classification, derived from SetupStatus.docker. */
type DockerVerdict = "present" | "absent" | "too_old";

/** Minimum Docker engine major version we install against. */
const MIN_DOCKER_MAJOR = 20;

function classifyDocker(docker: SetupStatus["docker"]): DockerVerdict {
  if (!docker.present) return "absent";
  const major = docker.version
    ? Number.parseInt(docker.version.split(".")[0] ?? "", 10)
    : NaN;
  if (!Number.isNaN(major) && major < MIN_DOCKER_MAJOR) return "too_old";
  return "present";
}

interface Props {
  clickhouse: SetupStatus["clickhouse"];
  docker: SetupStatus["docker"];
}

export function ClickHouseCard({ clickhouse, docker }: Props) {
  const router = useRouter();
  const verdict = classifyDocker(docker);
  const tone = stateTone(clickhouse.state);

  const [installing, setInstalling] = useState(false);
  const [step, setStep] = useState<InstallStep | string | null>(null);
  const [progress, setProgress] = useState(0);
  const [logLine, setLogLine] = useState<string | null>(null);
  const [installError, setInstallError] = useState<string | null>(null);
  const [done, setDone] = useState(false);
  const abortRef = useRef<AbortController | null>(null);

  async function handleInstall() {
    setInstalling(true);
    setInstallError(null);
    setDone(false);
    setProgress(0);
    setStep(null);
    setLogLine(null);
    const controller = new AbortController();
    abortRef.current = controller;
    try {
      const accepted = await postClickHouseInstall();
      await streamClickHouseInstall(
        accepted.stream,
        (e: InstallEvent) => {
          setStep(e.step);
          setProgress(Math.round(e.progress * 100));
          setLogLine(e.log_line);
          if (e.step === "complete") setDone(true);
        },
        controller.signal,
      );
      if (!controller.signal.aborted) {
        setDone(true);
        // Re-probe /setup/status so the section flips to its live state.
        router.refresh();
      }
    } catch (err) {
      setInstallError(
        err instanceof Error ? err.message : "Install failed.",
      );
    } finally {
      setInstalling(false);
    }
  }

  const pct = installing || done ? progress : 0;

  return (
    <div data-testid="clickhouse-card">
      <Card
        eyebrow="analytics store"
        title="ClickHouse"
        action={
          <span data-testid="ch-state-badge">
            <Pill tone={tone}>
              <span
                aria-hidden
                className="mr-[4px] inline-block h-[6px] w-[6px] rounded-full"
                style={{
                  background:
                    tone === "ok"
                      ? "var(--ok)"
                      : tone === "warn"
                        ? "var(--warn)"
                        : tone === "danger"
                          ? "var(--danger)"
                          : "var(--text-faint)",
                }}
              />
              {clickhouse.state}
            </Pill>
          </span>
        }
      >
        <div className="flex flex-col gap-[10px]">
          {clickhouse.url && <StatusRow label="URL" value={clickhouse.url} />}
          {clickhouse.error && (
            <p className="m-0 text-[length:var(--font-size-xs)] text-[color:var(--danger)]">
              {clickhouse.error}
            </p>
          )}

          {/* Docker detect result */}
          <div
            className="flex items-center justify-between gap-[8px]"
            data-testid="ch-docker-verdict"
            data-verdict={verdict}
          >
            <span className="text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
              Docker
            </span>
            {verdict === "present" && (
              <Pill tone="ok" mono={false}>
                {docker.version ? `v${docker.version}` : "available"}
              </Pill>
            )}
            {verdict === "too_old" && (
              <Pill tone="warn" mono={false}>
                {docker.version ? `v${docker.version} too old` : "too old"}
              </Pill>
            )}
            {verdict === "absent" && (
              <Pill tone="neutral" mono={false}>
                not found
              </Pill>
            )}
          </div>

          {/* Install action / progress (Docker present) */}
          {verdict === "present" && !done && (
            <div className="mt-[2px] flex items-center gap-[8px]">
              <Button
                variant="primary"
                size="sm"
                disabled={installing}
                onClick={() => void handleInstall()}
                data-testid="ch-install-button"
              >
                {installing ? "Installing…" : "Install locally"}
              </Button>
            </div>
          )}

          {(installing || done) && (
            <div data-testid="ch-install-progress">
              <div className="mb-[4px] flex items-center justify-between gap-[8px]">
                <span
                  className="text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]"
                  data-testid="ch-install-step"
                >
                  {step ? (STEP_LABELS[step] ?? step) : "Starting…"}
                </span>
                <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text)]">
                  {pct}%
                </span>
              </div>
              <div
                className="h-[6px] w-full overflow-hidden"
                style={{ background: "var(--hairline)" }}
                role="progressbar"
                aria-label="ClickHouse install progress"
                aria-valuenow={pct}
                aria-valuemin={0}
                aria-valuemax={100}
              >
                <div
                  className="h-full transition-[width] duration-300"
                  style={{
                    width: `${pct}%`,
                    background: done ? "var(--ok)" : "var(--info)",
                  }}
                />
              </div>
              {logLine && (
                <p
                  className="m-0 mt-[6px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]"
                  data-testid="ch-install-log"
                >
                  {logLine}
                </p>
              )}
            </div>
          )}

          {done && (
            <div data-testid="ch-install-complete">
              <Pill tone="ok" mono={false}>
                Install complete
              </Pill>
            </div>
          )}

          {installError && (
            <p
              className="m-0 text-[length:var(--font-size-xs)] text-[color:var(--danger)]"
              data-testid="ch-install-error"
            >
              {installError}
            </p>
          )}

          {/* Docker-absent / too-old → route to remote-connect (Story 15.4) */}
          {verdict !== "present" && (
            <div
              className="mt-[2px] flex flex-col gap-[6px]"
              data-testid="ch-remote-connect-cta"
            >
              <p className="m-0 text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
                {verdict === "absent"
                  ? "Docker isn't available on this host. Connect a managed ClickHouse instead."
                  : "Your Docker version is too old for the local install. Connect a managed ClickHouse instead."}
              </p>
              <Button
                variant="secondary"
                size="sm"
                onClick={() => router.push("/setup/clickhouse/connect")}
                data-testid="ch-remote-connect-button"
              >
                Connect remote ClickHouse
              </Button>
            </div>
          )}
        </div>
      </Card>
    </div>
  );
}

function StatusRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between gap-[8px]">
      <span className="text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
        {label}
      </span>
      <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text)]">
        {value}
      </span>
    </div>
  );
}
