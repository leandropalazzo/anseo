"use client";

import { useState } from "react";

import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Pill } from "@/components/ui/pill";
import { CapabilityInspector } from "@/components/dev/capability-inspector";
import type { DevPluginState, PluginLogLine } from "@/lib/dev-mode";

// UX-DR121 — hot-reload is atomic: the button is disabled mid-reload and the
// loaded version flips in a single step. UX-DR125 — in-flight invocations
// complete on the *old* version; we surface the preserved count during reload.
// UX-DR122 — the logs viewer is append-only: lines are only ever added.

function bumpVersion(v: string): string {
  // 0.1.0-dev+abc1234 → 0.1.0-dev+<new build hash>
  const base = v.split("+")[0];
  const hash = Math.random().toString(16).slice(2, 9);
  return `${base}+${hash}`;
}

export function DevOverview({ state }: { state: DevPluginState }) {
  const [version, setVersion] = useState(state.loaded_version);
  const [logs, setLogs] = useState<PluginLogLine[]>(state.logs);
  const [reloading, setReloading] = useState(false);
  const inFlight = state.in_flight_invocations;

  async function hotReload() {
    setReloading(true);
    // Append (never replace) a log line marking the reload start.
    const draining: PluginLogLine = {
      at: new Date().toISOString(),
      level: "info",
      message: `hot-reload: ${inFlight} in-flight invocation(s) draining on ${version}`,
    };
    setLogs((prev) => [...prev, draining]);

    await new Promise((r) => setTimeout(r, 50));

    const next = bumpVersion(version);
    setVersion(next); // atomic version flip
    setLogs((prev) => [
      ...prev,
      {
        at: new Date().toISOString(),
        level: "info",
        message: `hot-reload complete: now serving ${next}`,
      },
    ]);
    setReloading(false);
  }

  return (
    <div className="flex flex-col gap-[12px]">
      <Card>
        <div className="flex items-center justify-between gap-[12px]">
          <div className="flex flex-col gap-[4px]">
            <div className="label-eyebrow text-[color:var(--text-faint)]">
              loaded plugin
            </div>
            <div className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-sm)] text-[color:var(--text)]">
              {state.plugin_slug}
            </div>
            <div className="flex items-center gap-[6px]">
              <Pill mono>
                <span data-testid="dev-loaded-version">{version}</span>
              </Pill>
              {reloading && (
                <span data-testid="dev-in-flight">
                  <Pill mono tone="info">
                    {inFlight} in-flight on old version
                  </Pill>
                </span>
              )}
            </div>
          </div>
          <Button
            variant="primary"
            size="sm"
            data-testid="dev-hot-reload"
            disabled={reloading}
            onClick={() => void hotReload()}
          >
            {reloading ? "Reloading…" : "Hot-reload"}
          </Button>
        </div>
      </Card>

      <Card>
        <CapabilityInspector capabilities={state.capabilities} />
      </Card>

      <Card>
        <div className="label-eyebrow mb-[6px] text-[color:var(--text-faint)]">
          logs (append-only)
        </div>
        <ul
          data-testid="dev-logs"
          className="m-0 flex max-h-[280px] list-none flex-col gap-[2px] overflow-auto p-0 font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)]"
        >
          {logs.map((l, i) => (
            <li
              key={`${l.at}-${i}`}
              data-testid="dev-log-line"
              data-level={l.level}
              className="text-[color:var(--text-muted)]"
            >
              <span className="text-[color:var(--text-faint)]">{l.at}</span>{" "}
              [{l.level}] {l.message}
            </li>
          ))}
        </ul>
      </Card>
    </div>
  );
}
