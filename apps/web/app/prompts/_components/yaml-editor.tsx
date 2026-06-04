import type { MockPrompt } from "@/lib/mock";

import { DiffLine } from "./diff-line";

export interface YamlEditorProps {
  prompt: MockPrompt;
  showDiff: boolean;
}

function scheduleToCron(schedule: string): string {
  if (schedule.includes("6h")) return "0 */6 * * *";
  if (schedule.includes("12h")) return "0 */12 * * *";
  if (schedule.includes("4h")) return "0 */4 * * *";
  if (schedule === "daily") return "0 0 * * *";
  if (schedule === "weekly") return "0 0 * * 0";
  return schedule;
}

function buildYaml(prompt: MockPrompt): string {
  return `name: ${prompt.name}
text: "${prompt.text}"
providers:
  - openai
  - anthropic
  - gemini
  - perplexity
brand: pinecone
competitors:
  - qdrant
  - weaviate
  - milvus
  - chroma
  - lancedb
extractors:
  mentions: { mode: list-detect }
  citations: { domains: true, source-type: true }
schedule: ${scheduleToCron(prompt.schedule)}
threshold:
  brand_rank_lte: 3
  alert_on_drop_pp: 15
tags: [${prompt.tags.join(", ")}]`;
}

function YamlLine({ text }: { text: string }) {
  const m = /^(\s*)([\w-]+):(.*)$/.exec(text);
  if (m) {
    return (
      <div>
        <span>{m[1]}</span>
        <span style={{ color: "var(--info)" }}>{m[2]}</span>
        <span style={{ color: "var(--text-faint)" }}>:</span>
        <span>{m[3]}</span>
      </div>
    );
  }
  return <div>{text}</div>;
}

/**
 * Read-only YAML view of `prompts/<name>.yaml`. When `showDiff` is true,
 * renders a unified diff illustrating the change set against the working
 * tree (pure mock — no save action wired in v1).
 */
export function YamlEditor({ prompt, showDiff }: YamlEditorProps) {
  if (showDiff) {
    return (
      <div className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-sm)] leading-[1.7]">
        <DiffLine type="ctx" text={`name: ${prompt.name}`} />
        <DiffLine type="ctx" text={`text: "${prompt.text}"`} />
        <DiffLine type="ctx" text="providers:" />
        <DiffLine type="ctx" text="  - openai" />
        <DiffLine type="ctx" text="  - anthropic" />
        <DiffLine type="add" text="  - gemini" />
        <DiffLine type="add" text="  - perplexity" />
        <DiffLine type="ctx" text="brand: pinecone" />
        <DiffLine type="ctx" text="competitors:" />
        <DiffLine type="rem" text="  - milvus" />
        <DiffLine type="add" text="  - milvus" />
        <DiffLine type="add" text="  - lancedb" />
        <DiffLine type="ctx" text="threshold:" />
        <DiffLine type="rem" text="  brand_rank_lte: 5" />
        <DiffLine type="add" text="  brand_rank_lte: 3" />
        <DiffLine type="add" text="  alert_on_drop_pp: 15" />
      </div>
    );
  }

  const yaml = buildYaml(prompt);
  const lines = yaml.split("\n");
  return (
    <div className="grid grid-cols-[32px_1fr] font-[family-name:var(--font-mono)] text-[length:var(--font-size-sm)] leading-[1.7]">
      <div className="border-r border-[color:var(--hairline)] pr-[8px] text-right text-[color:var(--text-faint)]">
        {lines.map((_, i) => (
          <div key={i}>{i + 1}</div>
        ))}
      </div>
      <pre className="m-0 overflow-auto whitespace-pre pl-[12px] text-[color:var(--text)]">
        {lines.map((line, i) => (
          <YamlLine key={i} text={line} />
        ))}
      </pre>
    </div>
  );
}
