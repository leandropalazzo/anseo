"use client";

import { Card } from "@/components/ui/card";
import { CodeBlock } from "@/components/ui/code-block";
import { CLUSTER_HEALTH } from "@/lib/mock-ops";

export function DeploySection() {
  return (
    <div className="flex flex-col gap-[12px]">
      <Card eyebrow="active mode" title="Local · Docker Compose">
        <CodeBlock
          lang="yaml"
          code={`services:
  api:      { image: opengeo/api:0.4.2, ports: [8080] }
  worker:   { image: opengeo/worker:0.4.2 }
  postgres: { image: postgres:16, volumes: [opengeo-data:/var/lib/postgresql/data] }
  web:      { image: opengeo/web:0.4.2, ports: [3000] }`}
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
          <div className="flex flex-col gap-[8px]">
            {CLUSTER_HEALTH.map((s) => (
              <div
                key={s.svc}
                className="grid items-center gap-[8px] [grid-template-columns:10px_100px_1fr_80px]"
              >
                <span
                  className="inline-block h-[8px] w-[8px] rounded-full"
                  style={{
                    background: s.ok ? "var(--ok)" : "var(--danger)",
                    boxShadow: s.ok ? "0 0 8px var(--ok)" : "none",
                  }}
                />
                <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-sm)] text-[color:var(--text)]">
                  {s.svc}
                </span>
                <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
                  {s.latency}
                </span>
                <span className="text-right font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-muted)]">
                  {s.qps}
                </span>
              </div>
            ))}
          </div>
        </Card>
      </div>
    </div>
  );
}
