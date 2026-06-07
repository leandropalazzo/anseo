"use client";

import type { ReactNode } from "react";
import { Check, X } from "lucide-react";

import { Card } from "@/components/ui/card";
import { CodeBlock } from "@/components/ui/code-block";
import { ICON_DEFAULTS } from "@/lib/icons";
import { isBenchmarkTermsFinalized } from "@/lib/dev-mode";

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

// Story 44.1 AC5 — the identified-tier toggle. Disabled with a "terms in
// review" notice until Epic 39.7 finalizes the brand-visibility terms; enabled
// once `NEXT_PUBLIC_ANSEO_BENCHMARK_TERMS_FINALIZED=1`. The actual opt-in is
// driven by the OSS client (`ogeo benchmark optin --brand-visibility`); this
// surface reflects + gates the state. APPEARING ≠ CLAIMING.
function BrandVisibilityToggle() {
  const termsFinalized = isBenchmarkTermsFinalized();
  return (
    <Card
      eyebrow="benchmark · identified tier"
      title="Brand-visibility (identified) contribution"
    >
      <div className="flex items-start justify-between gap-[16px]">
        <div className="text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
          <p>
            Opt in to appear named and ranked in the public visibility
            leaderboard. Only your verified-domain token is transmitted — never
            your brand name. Separate from, and revocable independently of,
            anonymous aggregate contribution. APPEARING ≠ CLAIMING.
          </p>
          {!termsFinalized && (
            <p
              className="mt-[6px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)]"
              style={{ color: "var(--text-faint)" }}
              data-testid="brand-visibility-terms-notice"
            >
              terms in review — identified-tier terms are not yet finalized.
            </p>
          )}
        </div>
        <button
          type="button"
          role="switch"
          aria-checked={false}
          aria-label="Brand-visibility identified tier"
          disabled={!termsFinalized}
          data-testid="brand-visibility-toggle"
          className="shrink-0 rounded-[var(--radius-sm)] border border-[color:var(--hairline)] px-[10px] py-[4px] text-[length:var(--font-size-xs)] disabled:cursor-not-allowed disabled:opacity-50"
        >
          {termsFinalized ? "Enable" : "Unavailable"}
        </button>
      </div>
    </Card>
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
            label="No telemetry to Anseo"
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
database:  postgres://anseo@localhost:5432/anseo_test
exports:   ./reports/ (markdown, json, csv)
telemetry: <none>`}
        />
      </Card>
      <BrandVisibilityToggle />
    </div>
  );
}
