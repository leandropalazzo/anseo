"use client";

import type { McpCallRow } from "@/lib/api";
import { Card } from "@/components/ui/card";

export function ActivityLog({ calls }: { calls: McpCallRow[] }) {
  if (calls.length === 0) {
    return (
      <Card eyebrow="recent calls" title="Activity log">
        <p className="text-[length:var(--font-size-sm)] text-[color:var(--text-faint)]">
          No tool calls recorded yet. Start the MCP server with{" "}
          <code className="font-[family-name:var(--font-mono)]">ogeo mcp serve</code>{" "}
          and connect Claude Desktop.
        </p>
      </Card>
    );
  }

  return (
    <Card eyebrow={`${calls.length} recent calls`} title="Activity log" padding={false}>
      <table className="w-full border-collapse text-[length:var(--font-size-sm)]">
        <thead>
          <tr className="border-b border-[color:var(--hairline)] bg-[color:var(--bg-sunken)]">
            <th className="px-[12px] py-[6px] text-left label-eyebrow text-[color:var(--text-faint)]">Tool</th>
            <th className="px-[12px] py-[6px] text-left label-eyebrow text-[color:var(--text-faint)]">Status</th>
            <th className="px-[12px] py-[6px] text-right label-eyebrow text-[color:var(--text-faint)]">Latency</th>
            <th className="px-[12px] py-[6px] text-left label-eyebrow text-[color:var(--text-faint)]">Called</th>
          </tr>
        </thead>
        <tbody>
          {calls.map((row) => (
            <tr key={row.id} className="border-b border-[color:var(--hairline)] hover:bg-[color:var(--bg-elev-2)]">
              <td className="px-[12px] py-[8px] font-[family-name:var(--font-mono)] text-[color:var(--text)]">
                <a href={`/mcp/${row.tool_name}`} className="hover:text-[color:var(--accent)]">{row.tool_name}</a>
              </td>
              <td className="px-[12px] py-[8px]">
                <span style={{ color: row.status === "ok" ? "var(--ok)" : "var(--danger)" }}>
                  {row.status}
                </span>
              </td>
              <td className="px-[12px] py-[8px] text-right font-[family-name:var(--font-mono)] text-[color:var(--text-muted)]">
                {row.latency_ms}ms
              </td>
              <td className="px-[12px] py-[8px] text-[color:var(--text-faint)]">
                {new Date(row.called_at).toLocaleTimeString()}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </Card>
  );
}
