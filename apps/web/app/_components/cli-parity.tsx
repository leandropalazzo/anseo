import Link from "next/link";
import { ArrowRight, Bot, Globe, Terminal } from "lucide-react";

import { Card } from "@/components/ui/card";
import { CodeBlock } from "@/components/ui/code-block";

/**
 * CLI/Web/MCP parity strip. Reinforces the "same operation, three surfaces"
 * differentiator (see ANALYSIS §1 prototype goals).
 */
export function CliParity() {
  return (
    <Card
      eyebrow="parity"
      title="CLI ⇄ Web ⇄ MCP"
      accent
      action={
        <Link
          href="/mcp"
          className="inline-flex items-center gap-[5px] border border-[color:var(--border)] px-[8px] py-[4px] text-[length:var(--font-size-sm)] text-[color:var(--text-muted)] hover:text-[color:var(--text)]"
        >
          Open MCP <ArrowRight size={11} strokeWidth={1.5} />
        </Link>
      }
    >
      <div className="grid grid-cols-3 gap-[12px]">
        <div>
          <ParityLabel icon={<Terminal size={11} strokeWidth={1.5} />}>CLI</ParityLabel>
          <CodeBlock
            lang="bash"
            code={`ogeo prompt run --prompt vector-db
ogeo report generate --window 7d`}
          />
        </div>
        <div>
          <ParityLabel icon={<Globe size={11} strokeWidth={1.5} />}>Web</ParityLabel>
          <div className="border border-[color:var(--border)] bg-[color:var(--bg-sunken)] p-[10px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-muted)]">
            <div>→ overview / runs / visibility</div>
            <div>→ ⌘K command palette</div>
            <div>→ deep links to every run</div>
          </div>
        </div>
        <div>
          <ParityLabel icon={<Bot size={11} strokeWidth={1.5} />}>MCP (agent)</ParityLabel>
          <CodeBlock
            lang="json"
            code={`{ "tool": "get_visibility",
  "prompt": "vector-db", "days": 7 }`}
          />
        </div>
      </div>
    </Card>
  );
}

function ParityLabel({
  icon,
  children,
}: {
  icon: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div className="label-eyebrow mb-[6px] inline-flex items-center gap-[6px] text-[color:var(--text-faint)]">
      {icon}
      {children}
    </div>
  );
}
