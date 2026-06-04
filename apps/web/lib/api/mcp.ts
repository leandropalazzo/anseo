// Story 16.8 MCP dashboard real.

import { getJson } from "./_client";

export interface McpToolInfo {
  id: string;
  sig: string;
  doc: string;
  category: "visibility" | "runs" | "analytics" | "search";
}

export interface McpCallRow {
  id: string;
  tool_name: string;
  status: "ok" | "error";
  latency_ms: number;
  error_kind: string | null;
  called_at: string;
}

export interface McpToolStats {
  tool_name: string;
  total_calls: number;
  ok_calls: number;
  error_calls: number;
  error_rate: number;
  p50_ms: number | null;
  p95_ms: number | null;
}

export async function fetchMcpTools(): Promise<{ tools: McpToolInfo[] }> {
  return getJson<{ tools: McpToolInfo[] }>("/v1/mcp/tools");
}

export async function fetchMcpCalls(limit = 20): Promise<{ calls: McpCallRow[] }> {
  return getJson<{ calls: McpCallRow[] }>(`/v1/mcp/calls?limit=${limit}`);
}

export async function fetchMcpToolStats(tool: string): Promise<McpToolStats> {
  return getJson<McpToolStats>(`/v1/mcp/stats?tool=${encodeURIComponent(tool)}`);
}
