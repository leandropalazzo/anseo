import { ProviderDot } from "@/components/ui/provider-dot";
import type { ProviderId } from "@/lib/provider-colors";

/**
 * One competitor's per-provider win/loss roll-up, derived in `page.tsx` from
 * the live `/comparisons` matrix (`rows[].cells[]`).
 */
export interface WinLossRow {
  competitor: string;
  /** True when the competitor is ahead of us on this provider. */
  ahead: Readonly<Record<ProviderId, boolean>>;
  /** Human-readable list of providers where this competitor wins, or "—". */
  whereTheyWin: string;
}

export interface WinLossTableProps {
  rows: ReadonlyArray<WinLossRow>;
}

const PROVIDERS: ReadonlyArray<ProviderId> = [
  "openai",
  "anthropic",
  "gemini",
  "perplexity",
];

function hdrCls() {
  return "border-b border-[color:var(--border)] px-[12px] py-[6px] text-left font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] font-medium uppercase tracking-[0.4px] text-[color:var(--text-faint)]";
}
function tdCls() {
  return "whitespace-nowrap px-[12px] py-[6px] text-[length:var(--font-size-sm)] text-[color:var(--text)]";
}

export function WinLossTable({ rows }: WinLossTableProps) {
  return (
    <div className="overflow-auto">
      <table className="w-full border-collapse">
        <thead>
          <tr>
            <th className={hdrCls()}>competitor</th>
            {PROVIDERS.map((p) => (
              <th key={p} className={hdrCls()}>
                <span className="inline-flex items-center gap-[6px]">
                  <ProviderDot provider={p} />
                  {p}
                </span>
              </th>
            ))}
            <th className={hdrCls()}>where they win</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr
              key={row.competitor}
              className="border-b border-[color:var(--hairline)]"
            >
              <td className={tdCls()}>{row.competitor}</td>
              {PROVIDERS.map((p) => (
                <td key={p} className={tdCls()}>
                  <span
                    className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)]"
                    style={{
                      color: row.ahead[p]
                        ? "var(--danger)"
                        : "var(--text-faint)",
                    }}
                  >
                    {row.ahead[p] ? "ahead of us" : "behind us"}
                  </span>
                </td>
              ))}
              <td className={tdCls()}>
                <span className="text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
                  {row.whereTheyWin}
                </span>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
