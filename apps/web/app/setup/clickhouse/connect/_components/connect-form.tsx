"use client";

import { useState, useTransition } from "react";
import { useRouter } from "next/navigation";
import { Card } from "@/components/ui/card";
import { Pill } from "@/components/ui/pill";
import { Button } from "@/components/ui/button";
import { postClickHouseConnect } from "@/lib/api";
import type {
  ConnectPreset,
  ConnectResult,
  ConnectState,
} from "@/lib/api";

interface PresetSpec {
  id: ConnectPreset;
  label: string;
  /** Canonical origin URL template auto-filled when the preset is picked
   *  (OQ-P3-23). The operator edits the placeholder segments. */
  endpoint: string;
}

const PRESETS: PresetSpec[] = [
  {
    id: "tinybird",
    label: "Tinybird",
    endpoint: "https://api.tinybird.co",
  },
  {
    id: "aiven",
    label: "Aiven",
    endpoint: "https://YOUR-SERVICE.aivencloud.com:12345",
  },
  {
    id: "clickhouse_cloud",
    label: "ClickHouse Cloud",
    endpoint: "https://YOUR-INSTANCE.clickhouse.cloud:8443",
  },
  { id: "custom", label: "Custom", endpoint: "" },
];

/** Human copy per failure state for the ErrorBanner. */
const ERROR_COPY: Record<Exclude<ConnectState, "connected">, string> = {
  invalid_credentials:
    "ClickHouse rejected those credentials. Check the username and password.",
  unreachable:
    "Couldn't reach that endpoint. Verify the origin URL, port, and network access.",
  schema_incompatible:
    "The endpoint responded but the probe query failed — it may not be a ClickHouse-compatible HTTP interface.",
  bad_request: "The endpoint must be a full http(s) origin URL.",
  persist_failed:
    "Connected, but the endpoint couldn't be saved to anseo.yaml. Check file permissions.",
};

const inputClass =
  "min-w-0 flex-1 border border-[color:var(--border)] bg-[color:var(--bg-elev-2)] px-[8px] py-[5px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text)] placeholder:text-[color:var(--text-faint)] disabled:opacity-50 outline-none focus:border-[color:var(--accent)]";

export function ConnectForm() {
  const router = useRouter();
  const [preset, setPreset] = useState<ConnectPreset>("clickhouse_cloud");
  const [endpoint, setEndpoint] = useState(
    PRESETS.find((p) => p.id === "clickhouse_cloud")!.endpoint,
  );
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [database, setDatabase] = useState("default");
  const [result, setResult] = useState<ConnectResult | null>(null);
  const [isPending, startTransition] = useTransition();

  function selectPreset(id: ConnectPreset) {
    setPreset(id);
    const spec = PRESETS.find((p) => p.id === id)!;
    // Auto-fill the canonical template; Custom clears it.
    setEndpoint(spec.endpoint);
    setResult(null);
  }

  function handleSubmit(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const trimmed = endpoint.trim();
    if (!trimmed) return;
    setResult(null);
    startTransition(async () => {
      const r = await postClickHouseConnect({
        preset,
        endpoint: trimmed,
        username: username.trim() || undefined,
        password: password || undefined,
        database: database.trim() || undefined,
      });
      setResult(r);
      if (r.ok) {
        // Success kicks the operator back to /setup, where the ETL flow
        // (Story 15.5) now reads the live endpoint.
        router.push("/setup");
        router.refresh();
      }
    });
  }

  return (
    <div data-testid="ch-connect-form">
      <Card eyebrow="analytics store" title="Connect remote ClickHouse">
        <form onSubmit={handleSubmit} className="flex flex-col gap-[14px]">
          {/* Preset radios */}
          <fieldset
            className="m-0 flex flex-col gap-[8px] border-0 p-0"
            data-testid="ch-connect-presets"
          >
            <legend className="mb-[2px] p-0 text-[length:var(--font-size-xs)] font-medium text-[color:var(--text-muted)]">
              Provider preset
            </legend>
            <div className="flex flex-wrap gap-[8px]">
              {PRESETS.map((p) => (
                <label
                  key={p.id}
                  data-testid={`ch-preset-${p.id}`}
                  className="flex cursor-pointer items-center gap-[6px] border border-[color:var(--hairline)] px-[10px] py-[5px] text-[length:var(--font-size-xs)] text-[color:var(--text)] has-[:checked]:border-[color:var(--accent)]"
                >
                  <input
                    type="radio"
                    name="preset"
                    value={p.id}
                    checked={preset === p.id}
                    onChange={() => selectPreset(p.id)}
                    disabled={isPending}
                  />
                  {p.label}
                </label>
              ))}
            </div>
          </fieldset>

          <Field label="Origin URL" htmlFor="ch-endpoint">
            <input
              id="ch-endpoint"
              data-testid="ch-endpoint-input"
              type="text"
              value={endpoint}
              onChange={(e) => setEndpoint(e.target.value)}
              placeholder="https://host:8443"
              required
              disabled={isPending}
              className={inputClass}
            />
          </Field>

          <Field label="Username" htmlFor="ch-username">
            <input
              id="ch-username"
              data-testid="ch-username-input"
              type="text"
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              placeholder="default"
              disabled={isPending}
              className={inputClass}
            />
          </Field>

          <Field label="Password" htmlFor="ch-password">
            <input
              id="ch-password"
              data-testid="ch-password-input"
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              disabled={isPending}
              className={inputClass}
            />
          </Field>

          <Field label="Database" htmlFor="ch-database">
            <input
              id="ch-database"
              data-testid="ch-database-input"
              type="text"
              value={database}
              onChange={(e) => setDatabase(e.target.value)}
              placeholder="default"
              disabled={isPending}
              className={inputClass}
            />
          </Field>

          <div className="flex items-center gap-[8px]">
            <Button
              type="submit"
              variant="primary"
              size="sm"
              disabled={isPending || endpoint.trim() === ""}
              data-testid="ch-connect-submit"
            >
              {isPending ? "Connecting…" : "Connect & save"}
            </Button>
            <span className="text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
              Password is used only to probe; it is never written to
              anseo.yaml.
            </span>
          </div>

          {/* ErrorBanner / success */}
          {result && !result.ok && (
            <div
              data-testid="ch-connect-error"
              data-state={result.state}
              role="alert"
              aria-live="assertive"
              className="border border-[color:var(--danger)] bg-[color-mix(in_oklch,var(--danger)_10%,transparent)] p-[10px] text-[length:var(--font-size-xs)] text-[color:var(--danger)]"
            >
              {ERROR_COPY[result.state as Exclude<ConnectState, "connected">] ??
                result.message}
            </div>
          )}
          {result && result.ok && (
            <div data-testid="ch-connect-success">
              <Pill tone="ok" mono={false}>
                Connected — saved {result.endpoint}
              </Pill>
            </div>
          )}
        </form>
      </Card>
    </div>
  );
}

function Field({
  label,
  htmlFor,
  children,
}: {
  label: string;
  htmlFor: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex flex-col gap-[6px]">
      <label
        htmlFor={htmlFor}
        className="text-[length:var(--font-size-xs)] font-medium text-[color:var(--text-muted)]"
      >
        {label}
      </label>
      {children}
    </div>
  );
}
