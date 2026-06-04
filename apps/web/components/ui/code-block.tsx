"use client";

import { useState } from "react";
import { Check, Copy } from "lucide-react";

import { ICON_DEFAULTS } from "@/lib/icons";

export interface CodeBlockProps {
  code: string;
  lang?: string;
  /** Render the copy button (default true). */
  copy?: boolean;
  className?: string;
}

/**
 * Copyable code block. The language label sits in an uppercase eyebrow
 * row alongside the copy affordance — matches the CLI snippet treatment
 * used across the prototype.
 */
export function CodeBlock({
  code,
  lang = "bash",
  copy = true,
  className,
}: CodeBlockProps) {
  const [copied, setCopied] = useState(false);

  const onCopy = async () => {
    try {
      await navigator.clipboard?.writeText(code);
      setCopied(true);
      setTimeout(() => setCopied(false), 1200);
    } catch {
      /* clipboard may be unavailable (insecure context); fail silently. */
    }
  };

  return (
    <div
      className={[
        "relative overflow-hidden",
        "border border-[color:var(--border)] bg-[color:var(--bg-sunken)]",
        className,
      ]
        .filter(Boolean)
        .join(" ")}
    >
      <div className="flex items-center justify-between border-b border-[color:var(--hairline)] px-[10px] py-[5px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
        <span className="uppercase tracking-[var(--ui-tracking)]">{lang}</span>
        {copy && (
          <button
            type="button"
            onClick={onCopy}
            className="inline-flex cursor-pointer items-center gap-1 border-0 bg-transparent p-[2px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-muted)] hover:text-[color:var(--text)]"
            aria-label={copied ? "Copied" : "Copy code"}
          >
            {copied ? (
              <Check
                size={11}
                strokeWidth={ICON_DEFAULTS.strokeWidth}
                color="var(--ok)"
              />
            ) : (
              <Copy size={11} strokeWidth={ICON_DEFAULTS.strokeWidth} />
            )}
            {copied ? "copied" : "copy"}
          </button>
        )}
      </div>
      <pre className="m-0 whitespace-pre-wrap px-[12px] py-[10px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-sm)] leading-[1.55] text-[color:var(--text)]">
        {code}
      </pre>
    </div>
  );
}
