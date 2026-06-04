"use client";

import { useSyncExternalStore } from "react";

const noopSubscribe = () => () => {};

export interface LocalTimeProps {
  /** RFC3339 / ISO timestamp. */
  iso: string;
  /** What to render: "time" → HH:MM, "datetime" → MM-DD HH:MM. */
  mode?: "time" | "datetime";
}

function fmt(iso: string, mode: "time" | "datetime"): string {
  const d = new Date(iso);
  const hh = String(d.getHours()).padStart(2, "0");
  const mm = String(d.getMinutes()).padStart(2, "0");
  if (mode === "time") return `${hh}:${mm}`;
  const mo = String(d.getMonth() + 1).padStart(2, "0");
  const da = String(d.getDate()).padStart(2, "0");
  return `${mo}-${da} ${hh}:${mm}`;
}

/**
 * Renders a timestamp in the viewer's local timezone. The server renders the
 * UTC value first (avoiding hydration mismatch via suppressHydrationWarning),
 * then the client swaps in local time after mount.
 */
export function LocalTime({ iso, mode = "time" }: LocalTimeProps) {
  const utc =
    mode === "time"
      ? iso.slice(11, 16)
      : iso.slice(5, 16).replace("T", " ");

  // Server (and first hydration) renders UTC; the client snapshot swaps in
  // local time post-hydration without a setState-in-effect.
  const label = useSyncExternalStore(
    noopSubscribe,
    () => fmt(iso, mode),
    () => utc,
  );

  return <span suppressHydrationWarning>{label}</span>;
}
