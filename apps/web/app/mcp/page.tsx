import { fetchMcpTools, fetchMcpCalls, type McpToolInfo, type McpCallRow } from "@/lib/api";
import { ToolBrowser } from "./_components/tool-browser";
import { ActivityLog } from "./_components/activity-log";

export default async function McpPage() {
  let tools: McpToolInfo[] = [];
  let recentCalls: McpCallRow[] = [];

  try {
    const [toolsResp, callsResp] = await Promise.all([
      fetchMcpTools(),
      fetchMcpCalls(20),
    ]);
    tools = toolsResp.tools;
    recentCalls = callsResp.calls;
  } catch {
    // Fallback to empty — shows skeleton state
  }

  return (
    <section data-testid="mcp-page" className="flex flex-col gap-[12px]">
      <header className="flex items-baseline justify-between">
        <div>
          <h1 className="m-0 text-[length:22px] font-normal tracking-[var(--display-tracking)] text-[color:var(--text)]">
            MCP Server
          </h1>
          <p className="m-0 mt-[2px] text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
            Tools exposed to AI agents over the Model Context Protocol — the
            same surfaces as the Web UI, served by the Anseo MCP server.
          </p>
        </div>
      </header>
      <ToolBrowser tools={tools} />
      <ActivityLog calls={recentCalls} />
    </section>
  );
}
