import Link from "next/link";

import { LocalTime } from "./local-time";
import { Pill } from "@/components/ui/pill";
import { ProviderDot } from "@/components/ui/provider-dot";

/** Slim row for the Overview recent-runs list (subset of the live
 *  `RunListRow` from `lib/api/runs.ts`). */
export interface RecentRunRow {
  id: string;
  prompt_name: string;
  provider: string;
  started_at: string;
  status: "ok" | "failed";
  error_kind: string | null;
}

export interface RecentRunsListProps {
  runs: ReadonlyArray<RecentRunRow>;
}

export function RecentRunsList({ runs }: RecentRunsListProps) {
  return (
    <div>
      {runs.map((r, i) => (
        <Link
          key={r.id}
          href={`/runs/${r.id}`}
          className="grid w-full grid-cols-[60px_12px_1fr_auto] items-center gap-[10px] px-[14px] py-[8px] text-left text-[color:var(--text)] hover:bg-[color:var(--bg-elev-2)]"
          style={{
            borderBottom:
              i === runs.length - 1 ? undefined : "1px solid var(--hairline)",
          }}
        >
          <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
            <LocalTime iso={r.started_at} />

          </span>
          <ProviderDot provider={r.provider} />
          <span className="min-w-0 overflow-hidden text-ellipsis whitespace-nowrap text-[length:var(--font-size-sm)]">
            {r.prompt_name}
          </span>
          {r.status === "ok" ? (
            <Pill tone="ok">ok</Pill>
          ) : (
            <Pill tone="danger">{r.error_kind ?? "failed"}</Pill>
          )}
        </Link>
      ))}
    </div>
  );
}
