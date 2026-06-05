"use client";

import { useSyncExternalStore } from "react";

type Theme = "light" | "dark";

function subscribe(cb: () => void): () => void {
  if (typeof document === "undefined") return () => {};
  const observer = new MutationObserver(cb);
  observer.observe(document.documentElement, {
    attributes: true,
    attributeFilter: ["data-theme"],
  });
  // Cross-tab sync: a sibling tab's localStorage write fires a `storage`
  // event in this tab. We mirror the new value onto data-theme so the
  // MutationObserver above picks it up and triggers the React re-render.
  const onStorage = (event: StorageEvent) => {
    if (event.key !== "anseo-theme") return;
    const next = event.newValue === "dark" ? "dark" : "light";
    if (document.documentElement.getAttribute("data-theme") !== next) {
      document.documentElement.setAttribute("data-theme", next);
    }
  };
  window.addEventListener("storage", onStorage);
  return () => {
    observer.disconnect();
    window.removeEventListener("storage", onStorage);
  };
}

function getSnapshot(): Theme {
  if (typeof document === "undefined") return "light";
  return document.documentElement.getAttribute("data-theme") === "dark"
    ? "dark"
    : "light";
}

function getServerSnapshot(): Theme {
  // Match the `dark` fallback in the layout's pre-hydration theme script so
  // the first paint label agrees with the default DOM state.
  return "dark";
}

export function ThemeToggle() {
  const theme = useSyncExternalStore(
    subscribe,
    getSnapshot,
    getServerSnapshot,
  );
  const next: Theme = theme === "dark" ? "light" : "dark";
  return (
    <button
      type="button"
      aria-label={`Switch to ${next} theme`}
      aria-pressed={theme === "dark"}
      data-testid="theme-toggle"
      onClick={() => {
        document.documentElement.setAttribute("data-theme", next);
        try {
          localStorage.setItem("anseo-theme", next);
        } catch {
          /* localStorage may be unavailable; theme still applies for this session */
        }
      }}
      className="inline-flex items-center border border-[color:var(--border)] bg-transparent px-[8px] py-[3px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-muted)] hover:text-[color:var(--text)]"
    >
      {theme === "dark" ? "Light" : "Dark"}
    </button>
  );
}
