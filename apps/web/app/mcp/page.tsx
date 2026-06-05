import { fetchMcpTools, fetchMcpCalls, type McpToolInfo, type McpCallRow } from "@/lib/api";
import { PageHeader } from "@/components/ui/page-header";
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
      <PageHeader
        title="MCP Server"
        description="Tools exposed to AI agents over the Model Context Protocol — the same surfaces as the Web UI, served by the Anseo MCP server."
      />
      <ToolBrowser tools={tools} />
      <ActivityLog calls={recentCalls} />
    </section>
  );
}
