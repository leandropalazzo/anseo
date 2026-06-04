"use client";

import type { ReactNode } from "react";

import { CommandPalette } from "@/components/shell/command-palette";
import { Sidebar } from "@/components/shell/sidebar";
import { Topbar } from "@/components/shell/topbar";
import { useCmdK } from "@/hooks/use-cmd-k";

/**
 * Top-level dashboard chrome. Pinned 224px sidebar + (sticky topbar +
 * scrolling main). Children render inside `<main>`.
 *
 * The Command Palette is owned at this level so a single ⌘K listener
 * covers the entire app and the dialog floats above every screen.
 */
export function AppShell({ children }: { children: ReactNode }) {
  const cmd = useCmdK();
  return (
    <div
      className="flex min-h-screen bg-[color:var(--bg)] font-[family-name:var(--font-body)] text-[length:var(--font-size-base)] text-[color:var(--text)]"
      style={{ letterSpacing: "var(--ui-tracking)" }}
    >
      <Sidebar />
      <div className="flex min-w-0 flex-1 flex-col">
        <Topbar onOpenPalette={() => cmd.setOpen(true)} />
        <main
          className="min-w-0 flex-1 px-[20px] pb-[60px] pt-[20px]"
          data-testid="app-main"
        >
          {children}
        </main>
      </div>
      <CommandPalette open={cmd.open} onClose={() => cmd.setOpen(false)} />
    </div>
  );
}
