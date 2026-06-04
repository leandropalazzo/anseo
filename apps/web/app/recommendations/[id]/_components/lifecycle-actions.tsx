"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";

import { Button } from "@/components/ui/button";
import {
  transitionRecommendation,
  type Recommendation,
  type RecommendationState,
} from "@/lib/api";

// UX-DR104 — Snooze and Dismiss are distinct affordances. Neither maps to a
// dedicated lifecycle state, so the UI maps Snooze → acknowledged (temporary
// quieting) and Dismiss → dismissed (terminal). UX-DR108 — marking Acted
// without an evidence URL flags the "did this work?" measurement loop.

export function LifecycleActions({ rec }: { rec: Recommendation }) {
  const router = useRouter();
  const [pending, setPending] = useState<RecommendationState | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [evidenceUrl, setEvidenceUrl] = useState("");

  async function run(to: RecommendationState, evidence_url?: string) {
    setPending(to);
    setError(null);
    try {
      await transitionRecommendation(rec.id, { to, evidence_url });
      router.refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setPending(null);
    }
  }

  const terminal = rec.state === "dismissed" || rec.state === "measured";

  return (
    <div
      data-testid="rec-lifecycle-actions"
      className="flex flex-col gap-[8px]"
    >
      <div className="flex flex-wrap items-center gap-[6px]">
        <Button
          size="sm"
          variant="secondary"
          disabled={terminal || pending !== null}
          data-testid="rec-action-snooze"
          onClick={() => run("acknowledged")}
        >
          Snooze
        </Button>
        <Button
          size="sm"
          variant="primary"
          disabled={terminal || pending !== null}
          data-testid="rec-action-acted"
          onClick={() => run("acted", evidenceUrl || undefined)}
        >
          Mark acted
        </Button>
        <Button
          size="sm"
          variant="danger"
          disabled={terminal || pending !== null}
          data-testid="rec-action-dismiss"
          onClick={() => run("dismissed")}
        >
          Dismiss
        </Button>
      </div>

      <input
        type="url"
        value={evidenceUrl}
        onChange={(e) => setEvidenceUrl(e.target.value)}
        placeholder="evidence URL (optional, for measurement loop)"
        data-testid="rec-evidence-url"
        className="border border-[color:var(--border)] bg-[color:var(--bg-elev-2)] px-[8px] py-[4px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text)]"
      />

      {/* UX-DR108 — acted without a measurement is surfaced as an open loop. */}
      {rec.state === "acted" && (
        <div
          data-testid="rec-measurement-flag"
          role="status"
          className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--warn)]"
        >
          acted — did this work? add measurement evidence to close the loop
        </div>
      )}

      {error && (
        <div
          data-testid="rec-action-error"
          role="alert"
          className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--danger)]"
        >
          {error}
        </div>
      )}
    </div>
  );
}
