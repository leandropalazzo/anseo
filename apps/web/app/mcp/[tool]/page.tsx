import Link from "next/link";
import { notFound } from "next/navigation";
import { fetchMcpToolStats, fetchMcpCalls, type McpCallRow } from "@/lib/api";
import { Card } from "@/components/ui/card";
import { PageHeader } from "@/components/ui/page-header";

const KNOWN_TOOLS = [
  "run_prompt",
  "get_visibility",
  "compare_brands",
  "get_citations",
  "list_trends",
  "search_benchmarks",
];

export default async function McpToolDetailPage({
  params,
}: {
  params: Promise<{ tool: string }>;
}) {
  const { tool } = await params;

  if (!KNOWN_TOOLS.includes(tool)) {
    notFound();
  }

  let stats;
  let recentCalls: McpCallRow[] = [];

  try {
    const [statsResp, callsResp] = await Promise.all([
      fetchMcpToolStats(tool),
      fetchMcpCalls(50),
    ]);
    stats = statsResp;
    recentCalls = callsResp.calls.filter((c) => c.tool_name === tool);
  } catch {
    // null stats — shows empty state
  }

  return (
    <section className="flex flex-col gap-[12px]">
      <PageHeader
        title={tool}
        description={<span className="label-eyebrow text-[color:var(--text-faint)]">MCP tools / {tool}</span>}
      />

      {/* Stats row */}
      <div className="grid grid-cols-4 gap-[12px]">
        {(
          [
            { label: "Total calls", value: stats ? String(stats.total_calls) : "—" },
            { label: "Error rate", value: stats ? `${(stats.error_rate * 100).toFixed(1)}%` : "—" },
            {
              label: "p50 latency",
              value: stats?.p50_ms != null ? `${Math.round(stats.p50_ms)}ms` : "—",
            },
            {
              label: "p95 latency",
              value: stats?.p95_ms != null ? `${Math.round(stats.p95_ms)}ms` : "—",
            },
          ] as const
        ).map(({ label, value }) => (
          <Card key={label} eyebrow={label}>
            <p className="m-0 text-[length:var(--font-size-base)] font-medium text-[color:var(--text)]">
              {value}
            </p>
          </Card>
        ))}
      </div>

      {/* Recent invocations */}
      <Card
        eyebrow={`${recentCalls.length} recent invocations`}
        title="Invocation history"
        padding={false}
      >
        {recentCalls.length === 0 ? (
          <p className="px-[12px] py-[10px] text-[length:var(--font-size-sm)] text-[color:var(--text-faint)]">
            No invocations recorded yet.
          </p>
        ) : (
          <table className="w-full border-collapse text-[length:var(--font-size-sm)]">
            <thead>
              <tr className="border-b border-[color:var(--hairline)] bg-[color:var(--bg-sunken)]">
                <th className="px-[12px] py-[6px] text-left label-eyebrow text-[color:var(--text-faint)]">
                  Status
                </th>
                <th className="px-[12px] py-[6px] text-right label-eyebrow text-[color:var(--text-faint)]">
                  Latency
                </th>
                <th className="px-[12px] py-[6px] text-left label-eyebrow text-[color:var(--text-faint)]">
                  Time
                </th>
                <th className="px-[12px] py-[6px] text-left label-eyebrow text-[color:var(--text-faint)]">
                  Error
                </th>
              </tr>
            </thead>
            <tbody>
              {recentCalls.map((row) => (
                <tr key={row.id} className="border-b border-[color:var(--hairline)]">
                  <td
                    className="px-[12px] py-[8px]"
                    style={{
                      color: row.status === "ok" ? "var(--ok)" : "var(--danger)",
                    }}
                  >
                    {row.status}
                  </td>
                  <td className="px-[12px] py-[8px] text-right font-[family-name:var(--font-mono)] text-[color:var(--text-muted)]">
                    {row.latency_ms}ms
                  </td>
                  <td className="px-[12px] py-[8px] text-[color:var(--text-faint)]">
                    {new Date(row.called_at).toLocaleTimeString()}
                  </td>
                  <td className="px-[12px] py-[8px] text-[color:var(--text-faint)] font-[family-name:var(--font-mono)]">
                    {row.error_kind ?? "—"}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Card>

      <div>
        <Link href="/mcp" className="text-[length:var(--font-size-sm)] text-[color:var(--accent)]">
          ← All tools
        </Link>
      </div>
    </section>
  );
}
