"use client";

import type { ReactNode } from "react";
import { Check, X } from "lucide-react";

import { Card } from "@/components/ui/card";
import { CodeBlock } from "@/components/ui/code-block";
import { ICON_DEFAULTS } from "@/lib/icons";

interface PostureLineProps {
  ok: boolean;
  label: ReactNode;
  detail: ReactNode;
}

function PostureLine({ ok, label, detail }: PostureLineProps) {
  return (
    <div className="grid grid-cols-[18px_1fr] gap-[8px] border-b border-[color:var(--hairline)] py-[8px]">
      <span style={{ color: ok ? "var(--ok)" : "var(--text-faint)" }}>
        {ok ? (
          <Check size={13} strokeWidth={ICON_DEFAULTS.strokeWidth} />
        ) : (
          <X size={13} strokeWidth={ICON_DEFAULTS.strokeWidth} />
        )}
      </span>
      <div>
        <div className="text-[length:var(--font-size-sm)] text-[color:var(--text)]">
          {label}
        </div>
        <div className="mt-[2px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-muted)]">
          {detail}
        </div>
      </div>
    </div>
  );
}

export function PrivacySection() {
  return (
    <div className="flex flex-col gap-[12px]">
      <Card
        accent
        eyebrow="phase-1 default"
        title="Localhost-first · zero telemetry"
      >
        <div className="grid grid-cols-2 gap-[12px]">
          <PostureLine
            ok
            label="Keys stay on this machine"
            detail="Resolved via OS keychain or age-encrypted file."
          />
          <PostureLine
            ok
            label="Raw responses stay in your DB"
            detail="postgres://localhost:5432/opengeo. Never leaves."
          />
          <PostureLine
            ok
            label="No telemetry to OpenGEO"
            detail="Phase-1 default; verified at startup."
          />
          <PostureLine
            ok
            label="Secret redaction in logs"
            detail="Secret type refuses Debug/Display/Serialize."
          />
          <PostureLine
            ok={false}
            label="SOC2 Type II"
            detail="n/a in local-only mode."
          />
          <PostureLine
            ok={false}
            label="Audit log"
            detail="n/a in local-only mode."
          />
        </div>
      </Card>
      <Card eyebrow="data residency" title="Where your data lives">
        <CodeBlock
          lang="text"
          code={`secrets:   $XDG_CONFIG_HOME/opengeo/secrets.age (0600)
database:  postgres://opengeo@localhost:5432/opengeo_test
exports:   ./reports/ (markdown, json, csv)
telemetry: <none>`}
        />
      </Card>
    </div>
  );
}
