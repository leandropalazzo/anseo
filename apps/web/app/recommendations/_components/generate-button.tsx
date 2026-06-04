"use client";

import { useRouter } from "next/navigation";
import { useState } from "react";

import { generateRecommendations } from "@/lib/api";

/** Run the recommendation engine from the UI (Story 19.6 parity with
 *  `ogeo recommend generate`). Persists results and refreshes the list. */
export function GenerateButton() {
  const router = useRouter();
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function run() {
    setBusy(true);
    setError(null);
    setMsg(null);
    try {
      const res = await generateRecommendations();
      setMsg(`+${res.inserted_count} new (${res.generated_count} evaluated)`);
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "generate failed");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex items-center gap-[8px]">
      {msg && (
        <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-muted)]">
          {msg}
        </span>
      )}
      {error && (
        <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--danger)]">
          {error}
        </span>
      )}
      <button
        type="button"
        onClick={run}
        disabled={busy}
        data-testid="rec-generate"
        className="border border-[color:var(--border-strong)] bg-[color:var(--accent)] px-[12px] py-[5px] text-[length:var(--font-size-sm)] text-[color:var(--accent-ink)] disabled:opacity-50"
      >
        {busy ? "Generating…" : "Generate"}
      </button>
    </div>
  );
}
