"use client";

import { useEffect, useState } from "react";

import { Card } from "@/components/ui/card";
import { CodeBlock } from "@/components/ui/code-block";

/** Shape of `GET /v1/serve/status` (Story 37.1), via the `/api/serve/status` proxy. */
interface ServeStatus {
  mode: "supervisor" | "standalone";
  tier: string;
  components: {
    api: { status: string };
    worker: { status: string };
  };
  boot_at?: string | null;
}

type Liveness = "running" | "unknown" | "down";

interface ServiceRow {
  svc: string;
  /** Liveness; `unknown` services have no live probe yet (clearly labeled). */
  state: Liveness;
  detail: string;
}

const DOT: Record<Liveness, { bg: string; glow: string }> = {
  running: { bg: "var(--ok)", glow: "0 0 8px var(--ok)" },
  down: { bg: "var(--danger)", glow: "none" },
  unknown: { bg: "var(--text-faint)", glow: "none" },
};

function liveness(status: string | undefined): Liveness {
  if (status === "running") return "running";
  if (status === "down" || status === "stopped") return "down";
  return "unknown";
}

export function DeploySection() {
  const [status, setStatus] = useState<ServeStatus | null>(null);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    let cancelled = false;
    fetch("/api/serve/status", { cache: "no-store" })
      .then((r) => (r.ok ? r.json() : Promise.reject(r.status)))
      .then((s: ServeStatus) => {
        if (!cancelled) setStatus(s);
      })
      .catch(() => {
        /* Backend unreachable — fall through to the "unknown" rows below. */
      })
      .finally(() => {
        if (!cancelled) setLoaded(true);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const tier = status?.tier ?? "local";
  const mode = status?.mode ?? "standalone";
  const modeTitle =
    mode === "supervisor"
      ? `${tier} · ogeo serve (supervisor)`
      : `${tier} · standalone API`;

  // api + worker are driven by the live supervisor health (Story 37.1). The
  // remaining services have no live probe yet, so they render as "unknown"
  // rather than faking a healthy state.
  const rows: ServiceRow[] = [
    {
      svc: "api",
      state: liveness(status?.components?.api?.status),
      detail: status ? status.components.api.status : loaded ? "unreachable" : "…",
    },
    {
      svc: "worker",
      state: liveness(status?.components?.worker?.status),
      detail: status ? status.components.worker.status : loaded ? "unreachable" : "…",
    },
    { svc: "postgres", state: "unknown", detail: "no live probe yet" },
    { svc: "scheduler", state: "unknown", detail: "no live probe yet" },
    { svc: "mcp", state: "unknown", detail: "no live probe yet" },
  ];

  return (
    <div className="flex flex-col gap-[12px]">
      <Card eyebrow="active mode" title={modeTitle}>
        <CodeBlock
          lang="yaml"
          code={`services:
  api:      { image: anseo/api:0.4.2, ports: [8080] }
  worker:   { image: anseo/worker:0.4.2 }
  postgres: { image: postgres:16, volumes: [anseo-data:/var/lib/postgresql/data] }
  web:      { image: anseo/web:0.4.2, ports: [3000] }`}
        />
      </Card>
      <div className="grid grid-cols-2 gap-[12px]">
        <Card eyebrow="commands" title="Manage from CLI">
          <CodeBlock
            lang="bash"
            code={`ogeo compose up
ogeo compose status
ogeo compose logs --service api`}
          />
        </Card>
        <Card eyebrow="health" title="Cluster health">
          <div className="flex flex-col gap-[8px]" data-testid="cluster-health">
            {rows.map((s) => (
              <div
                key={s.svc}
                data-state={s.state}
                className="grid items-center gap-[8px] [grid-template-columns:10px_100px_1fr]"
              >
                <span
                  className="inline-block h-[8px] w-[8px] rounded-full"
                  style={{ background: DOT[s.state].bg, boxShadow: DOT[s.state].glow }}
                />
                <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-sm)] text-[color:var(--text)]">
                  {s.svc}
                </span>
                <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
                  {s.detail}
                </span>
              </div>
            ))}
          </div>
        </Card>
      </div>
    </div>
  );
}
