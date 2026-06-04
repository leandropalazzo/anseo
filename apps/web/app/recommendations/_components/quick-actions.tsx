"use client";

import { useRouter } from "next/navigation";
import { useState } from "react";

import { transitionRecommendation, type RecommendationState } from "@/lib/api";

/**
 * Inline row shortcut to close the feedback loop without opening the detail
 * page. "Mark done" advances to `acted` — chaining through `acknowledged`
 * first when needed, since the lifecycle has no direct `surfaced -> acted`
 * edge. Evidence can still be added later on the detail page.
 */
export function QuickActions({
  id,
  state,
}: {
  id: string;
  state: RecommendationState;
}) {
  const router = useRouter();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const terminal = state === "acted" || state === "measured" || state === "dismissed";

  async function markDone() {
    setBusy(true);
    setError(null);
    try {
      if (state === "surfaced") {
        await transitionRecommendation(id, { to: "acknowledged" });
      }
      await transitionRecommendation(id, { to: "acted" });
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "failed");
      setBusy(false);
    }
  }

  async function dismiss() {
    setBusy(true);
    setError(null);
    try {
      await transitionRecommendation(id, { to: "dismissed" });
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "failed");
      setBusy(false);
    }
  }

  if (terminal) {
    return (
      <span className="shrink-0 self-center font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
        {state}
      </span>
    );
  }

  return (
    <div className="flex shrink-0 items-center gap-[6px] self-center">
      {error && (
        <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--danger)]">
          {error}
        </span>
      )}
      <button
        type="button"
        onClick={markDone}
        disabled={busy}
        data-testid="rec-quick-done"
        title="Mark acted — feeds the what-works intelligence"
        className="border border-[color:var(--border-strong)] bg-[color:var(--accent)] px-[8px] py-[3px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--accent-ink)] disabled:opacity-50"
      >
        {busy ? "…" : "Mark done"}
      </button>
      <button
        type="button"
        onClick={dismiss}
        disabled={busy}
        data-testid="rec-quick-dismiss"
        title="Dismiss"
        className="border border-[color:var(--border)] px-[8px] py-[3px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-muted)] hover:text-[color:var(--text)] disabled:opacity-50"
      >
        Dismiss
      </button>
    </div>
  );
}
