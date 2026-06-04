"use client";

import { useSyncExternalStore } from "react";

// UX-DR119 — the Dev-mode banner is unmistakable: full-width, high-contrast,
// always at the top of /dev. OQ-P3-28 — dismissal persists per-session only
// (sessionStorage), so it reappears in a fresh tab/session.

const DISMISS_KEY = "ogeo-dev-banner-dismissed";

const listeners = new Set<() => void>();
function subscribe(cb: () => void): () => void {
  listeners.add(cb);
  return () => listeners.delete(cb);
}
function getSnapshot(): boolean {
  return sessionStorage.getItem(DISMISS_KEY) === "1";
}
function getServerSnapshot(): boolean {
  return false;
}
function dismiss(): void {
  sessionStorage.setItem(DISMISS_KEY, "1");
  listeners.forEach((l) => l());
}

export function DevBanner() {
  const dismissed = useSyncExternalStore(
    subscribe,
    getSnapshot,
    getServerSnapshot,
  );
  if (dismissed) return null;

  return (
    <div
      role="status"
      data-testid="dev-mode-banner"
      className="flex items-center justify-between gap-[8px] border-2 border-[color:var(--warn)] bg-[color:color-mix(in_oklch,var(--warn)_14%,transparent)] px-[12px] py-[8px]"
    >
      <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-sm)] font-medium text-[color:var(--warn)]">
        ⚠ DEV MODE — plugin author surfaces are active. Hot-reload and unsigned
        local plugins are enabled.
      </span>
      <button
        type="button"
        data-testid="dev-banner-dismiss"
        onClick={dismiss}
        className="border border-[color:var(--warn)] px-[6px] py-[2px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--warn)]"
      >
        dismiss
      </button>
    </div>
  );
}
