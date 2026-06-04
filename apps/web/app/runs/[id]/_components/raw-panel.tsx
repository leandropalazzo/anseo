import { Card } from "@/components/ui/card";
import { CodeBlock } from "@/components/ui/code-block";
import type { RunDetail } from "@/lib/api";

export interface RawPanelProps {
  run: RunDetail;
}

export function RawPanel({ run }: RawPanelProps) {
  const pretty = (() => {
    try {
      return JSON.stringify(run, null, 2);
    } catch {
      return String(run);
    }
  })();
  return (
    <div className="grid grid-cols-[1.4fr_1fr] gap-[12px]">
      <Card eyebrow={`GET /api/runs/${run.id}`} title="Raw response">
        <CodeBlock lang="json" code={pretty} />
      </Card>
      <Card eyebrow="reproduce" title="Reproduce from CLI / MCP">
        <div className="flex flex-col gap-[10px]">
          <CodeBlock
            lang="bash"
            code={`ogeo prompt run --prompt ${run.prompt_name} --provider ${run.provider} --replay ${run.id}`}
          />
          <CodeBlock
            lang="bash"
            code={`curl http://localhost:8080/api/runs/${run.id} | jq`}
          />
          <CodeBlock
            lang="json"
            code={`{ "tool": "get_run",
  "id": "${run.id}" }`}
          />
        </div>
      </Card>
    </div>
  );
}
