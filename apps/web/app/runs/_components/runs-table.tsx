import Link from "next/link";

import { ProviderDot } from "@/components/ui/provider-dot";
import type { MockRun } from "@/lib/mock";
import {
  providerRunIdentity,
  resolveProviderIdentity,
} from "@/lib/provider-colors";

import { LocalTime } from "../../_components/local-time";
import { StatusPill } from "./status-pill";

export interface RunsTableProps {
  runs: ReadonlyArray<MockRun>;
}

const HEADERS: ReadonlyArray<string> = [
  "started",
  "id",
  "prompt",
  "provider",
  "model",
  "rank",
  "mentions",
  "latency",
  "tokens",
  "status",
];

export function RunsTable({ runs }: RunsTableProps) {
  return (
    <div className="overflow-auto">
      <table
        className="w-full border-collapse text-[length:var(--font-size-sm)]"
        data-testid="runs-table"
      >
        <thead>
          <tr className="bg-[color:var(--bg-sunken)]">
            {HEADERS.map((h) => (
              <th
                key={h}
                className="label-eyebrow border-b border-[color:var(--border)] px-[12px] py-[6px] text-left text-[color:var(--text-faint)]"
              >
                {h}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {runs.map((r) => {
            const providerIdentity = providerRunIdentity(
              r.provider,
              r.provider_model_version,
            );
            const providerLabel = resolveProviderIdentity(providerIdentity).label;
            return (
              <tr
                key={r.id}
                className="border-b border-[color:var(--hairline)] hover:bg-[color:var(--bg-elev-2)]"
              >
                <Td mono faint>
                  <LocalTime iso={r.started_at} mode="datetime" />
                </Td>
                <Td mono>
                  <Link
                    href={`/runs/${r.id}`}
                    className="text-[color:var(--text)] hover:text-[color:var(--accent)]"
                  >
                    {r.id.slice(0, 14)}
                  </Link>
                </Td>
                <Td>{r.prompt_name}</Td>
                <Td>
                  <span className="inline-flex items-center gap-[6px]">
                    <ProviderDot provider={providerIdentity} size={14} />
                    <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)]">
                      {providerLabel}
                    </span>
                  </span>
                </Td>
                <Td mono faint>
                  {r.provider_model_version}
                </Td>
                <Td mono>{r.brand_rank ?? "—"}</Td>
                <Td mono>{r.mentions}</Td>
                <Td mono>{(r.latency_ms / 1000).toFixed(2)}s</Td>
                <Td mono faint>
                  {r.tokens_in}+{r.tokens_out}
                </Td>
                <Td>
                  <StatusPill status={r.status} errorKind={r.error_kind} />
                </Td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

function Td({
  children,
  mono,
  faint,
}: {
  children: React.ReactNode;
  mono?: boolean;
  faint?: boolean;
}) {
  return (
    <td
      className="whitespace-nowrap px-[12px] py-[6px]"
      style={{
        fontFamily: mono ? "var(--font-mono)" : "var(--font-body)",
        fontSize: mono ? "var(--font-size-xs)" : "var(--font-size-sm)",
        color: faint ? "var(--text-muted)" : "var(--text)",
      }}
    >
      {children}
    </td>
  );
}
