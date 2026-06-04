import type { ReactNode } from "react";

/**
 * Onboarding route-group layout. Renders the wizard outside the AppShell
 * (the root layout's ShellGate detects `/onboarding` and skips the
 * sidebar/topbar/command-palette chrome).
 *
 * Locked to dark theme — the Signal direction is the brand at first
 * impression. Operators can switch themes from Settings after init.
 */
export default function OnboardingLayout({
  children,
}: {
  children: ReactNode;
}) {
  return (
    <div
      data-theme="dark"
      data-testid="onboarding-layout"
      className="min-h-screen bg-[color:var(--bg)] font-[family-name:var(--font-body)] text-[length:var(--font-size-base)] text-[color:var(--text)]"
      style={{ letterSpacing: "var(--ui-tracking)" }}
    >
      <div className="mx-auto max-w-[1200px] px-[24px] py-[40px]">
        {children}
      </div>
    </div>
  );
}
