import { Fragment } from "react";

const COMPETITORS: ReadonlyArray<string> = [
  "Pinecone",
  "Qdrant",
  "Weaviate",
  "Milvus",
  "Chroma",
  "LanceDB",
  "Turbopuffer",
];
const BRAND = "Pinecone";

const SPLIT_RE = new RegExp(`(\\b(?:${COMPETITORS.join("|")})\\b)`);

export interface HighlightedResponseProps {
  text: string;
  /** When false, render plain text (no inline highlights). */
  highlight?: boolean;
}

/**
 * Inline-highlights brand + competitor mentions inside a raw LLM response.
 * Brand uses the accent token; other competitors use the info token. Both
 * tokens are bg+fg pairs to ensure WCAG-AA contrast against `--bg-elev`.
 */
export function HighlightedResponse({
  text,
  highlight = true,
}: HighlightedResponseProps) {
  if (!highlight) return <span className="whitespace-pre-wrap">{text}</span>;
  const parts = text.split(SPLIT_RE);
  return (
    <span className="whitespace-pre-wrap">
      {parts.map((chunk, i) => {
        if (COMPETITORS.includes(chunk)) {
          const isBrand = chunk === BRAND;
          return (
            <span
              key={i}
              className="px-[3px] font-semibold"
              style={{
                background: isBrand
                  ? "var(--accent-soft)"
                  : "color-mix(in oklch, var(--info) 14%, transparent)",
                color: isBrand ? "var(--accent)" : "var(--info)",
              }}
            >
              {chunk}
            </span>
          );
        }
        return <Fragment key={i}>{chunk}</Fragment>;
      })}
    </span>
  );
}
