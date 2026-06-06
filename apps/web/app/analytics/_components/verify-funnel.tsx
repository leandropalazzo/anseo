import type { VerifyMethod } from "@/lib/api";
import { EmptyState } from "@/components/ui/empty-state";

/**
 * Verify funnel: start → complete / fail counts grouped by method (dns | email)
 * with a success rate. `success_rate_pct` is `null` when no starts were
 * recorded for the method (rendered "—" rather than a divide-by-zero NaN).
 *
 * Method names are server-provided enum strings rendered as React text
 * (auto-escaped).
 */
export function VerifyFunnel({ methods }: { methods: VerifyMethod[] }) {
  if (methods.length === 0) {
    return (
      <EmptyState
        title="No verification activity"
        hint="verify_start / verify_complete events will populate this once the public verify flow sees traffic"
      />
    );
  }
  return (
    <table className="w-full border-collapse" data-testid="verify-funnel">
      <thead>
        <tr className="border-b border-[color:var(--hairline)] text-left font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] uppercase tracking-[0.06em] text-[color:var(--text-faint)]">
          <th className="py-[6px] font-normal">Method</th>
          <th className="py-[6px] text-right font-normal">Start</th>
          <th className="py-[6px] text-right font-normal">Complete</th>
          <th className="py-[6px] text-right font-normal">Fail</th>
          <th className="py-[6px] text-right font-normal">Success</th>
        </tr>
      </thead>
      <tbody>
        {methods.map((m) => (
          <tr
            key={m.method}
            className="border-b border-[color:var(--hairline)] text-[length:var(--font-size-sm)] text-[color:var(--text)]"
            data-testid={`verify-row-${m.method}`}
          >
            <td className="py-[6px] font-[family-name:var(--font-mono)]">
              {m.method}
            </td>
            <td className="py-[6px] text-right tabular-nums">
              {m.start.toLocaleString()}
            </td>
            <td className="py-[6px] text-right tabular-nums">
              {m.complete.toLocaleString()}
            </td>
            <td className="py-[6px] text-right tabular-nums text-[color:var(--text-muted)]">
              {m.fail.toLocaleString()}
            </td>
            <td className="py-[6px] text-right tabular-nums">
              {m.success_rate_pct === null
                ? "—"
                : `${m.success_rate_pct.toFixed(1)}%`}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
