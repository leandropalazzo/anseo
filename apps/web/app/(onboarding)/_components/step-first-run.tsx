"use client";

import { useState } from "react";
import { ArrowRight, Play } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { CodeBlock } from "@/components/ui/code-block";
import { ICON_DEFAULTS } from "@/lib/icons";
import { FIRST_RUN_LOG } from "@/lib/mock-ops";

export interface StepFirstRunProps {
  onNext: () => void;
}

const INPUT_CLASS =
  "mt-[6px] w-full border border-[color:var(--border)] bg-[color:var(--bg-sunken)] px-[10px] py-[8px] text-[length:var(--font-size-sm)] text-[color:var(--text)] outline-0 font-[family-name:var(--font-mono)]";

export function StepFirstRun({ onNext }: StepFirstRunProps) {
  const [running, setRunning] = useState(false);
  const [done, setDone] = useState(false);

  const go = () => {
    setRunning(true);
    window.setTimeout(() => {
      setDone(true);
      setRunning(false);
    }, 1400);
  };

  return (
    <Card eyebrow="step 4 · first prompt run" title="Let's run something">
      <div className="grid grid-cols-1 gap-[14px]">
        <div>
          <div className="label-eyebrow text-[color:var(--text-faint)]">
            prompt
          </div>
          <input
            defaultValue="best vector database for production RAG"
            className={INPUT_CLASS}
          />
        </div>
        <CodeBlock
          lang="bash"
          code={'ogeo prompt run --prompt "best vector database for production RAG"'}
        />
        <div
          className="min-h-[160px] border border-[color:var(--border)] bg-[color:var(--bg-sunken)] p-[12px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] leading-[1.7] text-[color:var(--text)]"
          data-testid="first-run-log"
        >
          {!running && !done ? (
            <span style={{ color: "var(--text-faint)" }}>
              Click &quot;Run&quot; to execute across all connected providers...
            </span>
          ) : (
            FIRST_RUN_LOG.slice(0, done ? FIRST_RUN_LOG.length : 3).map(
              (l, i) => (
                <div
                  key={i}
                  style={{
                    color: l.startsWith("✓") ? "var(--ok)" : "var(--text)",
                  }}
                >
                  {l}
                </div>
              ),
            )
          )}
        </div>
      </div>
      <div className="mt-[16px] flex items-center justify-between">
        <Button
          variant="ghost"
          size="sm"
          onClick={go}
          leadingIcon={
            <Play size={11} strokeWidth={ICON_DEFAULTS.strokeWidth} />
          }
        >
          Run
        </Button>
        <Button
          variant="primary"
          size="sm"
          onClick={onNext}
          disabled={!done}
          leadingIcon={
            <ArrowRight size={11} strokeWidth={ICON_DEFAULTS.strokeWidth} />
          }
        >
          Continue
        </Button>
      </div>
    </Card>
  );
}
