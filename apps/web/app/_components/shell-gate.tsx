"use client";

import type { ReactNode } from "react";
import { usePathname } from "next/navigation";

import { AppShell } from "@/components/shell/app-shell";

/**
 * Conditionally wraps `children` in `<AppShell>`. The onboarding route
 * (`/onboarding`) renders bare — no sidebar, no topbar, no command
 * palette — so the first-run wizard owns the viewport.
 *
 * Lives in `_components/` so it stays a private app-router helper; not
 * a public component primitive.
 */
export function ShellGate({ children }: { children: ReactNode }) {
  const pathname = usePathname();
  const bare = pathname === "/onboarding" || pathname?.startsWith("/onboarding/");
  if (bare) return <>{children}</>;
  return <AppShell>{children}</AppShell>;
}
