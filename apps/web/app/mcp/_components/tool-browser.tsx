"use client";

import { useMemo, useState } from "react";
import { Search } from "lucide-react";

import { Card } from "@/components/ui/card";
import { CodeBlock } from "@/components/ui/code-block";
import { ICON_DEFAULTS } from "@/lib/icons";
import type { McpToolInfo } from "@/lib/api";

type Category = McpToolInfo["category"];

const CATEGORY_LABEL: Readonly<Record<Category, string>> = {
  visibility: "Visibility",
  runs: "Runs",
  analytics: "Analytics",
  search: "Search",
};

function exampleCall(toolId: string): string {
  if (toolId === "get_visibility") {
    return `{ "tool": "${toolId}", "args": { "prompt": "vector-db", "days": 7 } }`;
  }
  return `{ "tool": "${toolId}", "args": {} }`;
}

const CLAUDE_DESKTOP_CONFIG = `{
  "mcpServers": {
    "anseo": {
      "command": "ogeo",
      "args": ["mcp", "serve"]
    }
  }
}`;

export function ToolBrowser({ tools }: { tools: McpToolInfo[] }) {
  const [toolId, setToolId] = useState<string>(
    tools.length > 0 ? tools[0]!.id : "get_visibility",
  );
  const [query, setQuery] = useState<string>("");

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return tools;
    return tools.filter(
      (t) =>
        t.id.toLowerCase().includes(q) ||
        t.doc.toLowerCase().includes(q) ||
        t.sig.toLowerCase().includes(q),
    );
  }, [query, tools]);

  const grouped = useMemo(() => {
    const out: Record<Category, McpToolInfo[]> = {
      visibility: [],
      runs: [],
      analytics: [],
      search: [],
    };
    for (const t of filtered) out[t.category].push(t);
    return out;
  }, [filtered]);

  const tool = tools.find((x) => x.id === toolId) ?? tools[0];

  if (!tool) {
    return (
      <Card eyebrow="0 tools exposed" title="MCP tools">
        <p className="text-[length:var(--font-size-sm)] text-[color:var(--text-faint)]">
          No tools available — check that the API server is running.
        </p>
      </Card>
    );
  }

  return (
    <div className="grid gap-[12px] [grid-template-columns:240px_1fr]">
      <Card padding={false} eyebrow={`${tools.length} tools exposed`} title="MCP tools">
        <div className="flex items-center gap-[6px] border-b border-[color:var(--hairline)] px-[10px] py-[6px]">
          <Search
            size={11}
            strokeWidth={ICON_DEFAULTS.strokeWidth}
            color="var(--text-faint)"
          />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="filter"
            className="flex-1 border-0 bg-transparent font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text)] outline-0"
          />
        </div>
        <div className="flex flex-col">
          {(Object.keys(grouped) as Category[]).map((cat) => {
            const list = grouped[cat];
            if (list.length === 0) return null;
            return (
              <div key={cat}>
                <div className="border-b border-[color:var(--hairline)] bg-[color:var(--bg-sunken)] px-[12px] py-[4px] label-eyebrow text-[color:var(--text-faint)]">
                  {CATEGORY_LABEL[cat]}
                </div>
                {list.map((x) => {
                  const active = toolId === x.id;
                  return (
                    <button
                      key={x.id}
                      type="button"
                      onClick={() => setToolId(x.id)}
                      className={[
                        "cursor-pointer appearance-none border-0 border-b border-[color:var(--hairline)] px-[12px] py-[8px] text-left",
                        active
                          ? "bg-[color:var(--bg-elev-2)]"
                          : "bg-transparent hover:bg-[color:var(--bg-elev-2)]",
                      ].join(" ")}
                      style={{
                        borderLeft: active
                          ? "2px solid var(--accent)"
                          : "2px solid transparent",
                      }}
                      data-testid={`mcp-tool-${x.id}`}
                    >
                      <div className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-sm)] text-[color:var(--text)]">
                        {x.id}
                      </div>
                      <div className="mt-[2px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
                        {x.sig}
                      </div>
                    </button>
                  );
                })}
              </div>
            );
          })}
        </div>
      </Card>

      <div className="flex flex-col gap-[12px]">
        <Card eyebrow={`tool · ${tool.id}`} title={tool.doc}>
          <div className="grid grid-cols-2 gap-[14px]">
            <div>
              <div className="mb-[6px] label-eyebrow text-[color:var(--text-faint)]">
                signature
              </div>
              <CodeBlock
                lang="typescript"
                copy={false}
                code={`function ${tool.id}${tool.sig}: Result`}
              />
            </div>
            <div>
              <div className="mb-[6px] label-eyebrow text-[color:var(--text-faint)]">
                example call
              </div>
              <CodeBlock lang="json" copy={false} code={exampleCall(tool.id)} />
            </div>
          </div>
        </Card>

        <Card eyebrow="setup" title="Connect over MCP">
          <p className="m-0 text-[length:var(--font-size-sm)] leading-[1.55] text-[color:var(--text-muted)]">
            These tools are served by the Anseo MCP server, not the web UI.
            Start the server, then register it with an MCP client such as
            Claude Desktop. Calls then appear in the activity log below.
          </p>
          <div className="mt-[12px] flex flex-col gap-[10px]">
            <div>
              <div className="mb-[6px] label-eyebrow text-[color:var(--text-faint)]">
                start the server
              </div>
              <CodeBlock lang="bash" code="ogeo mcp serve" />
            </div>
            <div>
              <div className="mb-[6px] label-eyebrow text-[color:var(--text-faint)]">
                claude_desktop_config.json
              </div>
              <CodeBlock lang="json" code={CLAUDE_DESKTOP_CONFIG} />
            </div>
          </div>
        </Card>
      </div>
    </div>
  );
}
