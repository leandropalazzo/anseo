"use client";

import { useRouter } from "next/navigation";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { NAV_GROUPS } from "@/components/shell/sidebar";
import { KBD } from "@/components/ui/kbd";
import { Icon, ICON_DEFAULTS } from "@/lib/icons";

type CommandKind = "nav" | "cli" | "action";

interface Command {
  id: string;
  label: string;
  kind: CommandKind;
  /** For CLI snippets — also shown in the secondary line. */
  cmd?: string;
  /** Optional sidebar shortcut hint (e.g. "G O"). */
  shortcut?: string;
  run: () => void;
}

interface CommandPaletteProps {
  open: boolean;
  onClose: () => void;
}

/**
 * ⌘K dialog. Hand-rolled (no Base UI Dialog) to match the prototype's
 * approach — keeps the SSR surface tiny and avoids importing the full
 * Base UI dialog tree solely for a backdrop + focus trap. Keyboard
 * shortcuts (⌘K to open, Esc to close) are owned by `useCmdK`.
 */
export function CommandPalette({ open, onClose }: CommandPaletteProps) {
  const router = useRouter();
  const [query, setQuery] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  // Focus the input when opened. Resetting the query is handled in the
  // wrapped close callback so we don't drive setState from an effect.
  useEffect(() => {
    if (!open) return undefined;
    const t = window.setTimeout(() => inputRef.current?.focus(), 30);
    return () => window.clearTimeout(t);
  }, [open]);

  const close = useCallback(() => {
    setQuery("");
    onClose();
  }, [onClose]);

  // Lock body scroll while open.
  useEffect(() => {
    if (!open) return undefined;
    const prev = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      document.body.style.overflow = prev;
    };
  }, [open]);

  const commands = useMemo<ReadonlyArray<Command>>(() => {
    const nav: Command[] = NAV_GROUPS.flatMap((g) =>
      g.items
        .filter((n) => !n.disabled)
        .map((n) => ({
          id: `nav:${n.href}`,
          label: `Go to ${n.label}`,
          kind: "nav" as const,
          shortcut: n.shortcut,
          run: () => {
            router.push(n.href);
            close();
          },
        })),
    );
    const cli: Command[] = [
      {
        id: "cli:run",
        label: "Run prompt",
        kind: "cli",
        cmd: "ogeo run --prompt vector-db",
        run: () => {
          void navigator.clipboard?.writeText("ogeo run --prompt vector-db");
          close();
        },
      },
      {
        id: "cli:schedule",
        label: "List schedules",
        kind: "cli",
        cmd: "ogeo schedule list",
        run: () => {
          void navigator.clipboard?.writeText("ogeo schedule list");
          close();
        },
      },
      {
        id: "cli:benchmark",
        label: "Run benchmark",
        kind: "cli",
        cmd: "ogeo benchmark",
        run: () => {
          void navigator.clipboard?.writeText("ogeo benchmark");
          close();
        },
      },
    ];
    const actions: Command[] = [
      {
        id: "action:toggle-theme",
        label: "Toggle theme",
        kind: "action",
        run: () => {
          const cur = document.documentElement.getAttribute("data-theme");
          const next = cur === "dark" ? "light" : "dark";
          document.documentElement.setAttribute("data-theme", next);
          try {
            localStorage.setItem("ogeo-theme", next);
          } catch {
            /* ignore */
          }
          close();
        },
      },
    ];
    return [...nav, ...cli, ...actions];
  }, [router, close]);

  const filtered = useMemo(() => {
    if (!query) return commands;
    const q = query.toLowerCase();
    return commands.filter(
      (c) =>
        c.label.toLowerCase().includes(q) ||
        (c.cmd ?? "").toLowerCase().includes(q),
    );
  }, [commands, query]);

  if (!open) return null;

  return (
    <div
      role="presentation"
      onClick={close}
      className="fixed inset-0 z-[100] flex items-start justify-center bg-[color-mix(in_oklch,var(--bg)_60%,transparent)] pt-[10vh] backdrop-blur-[6px]"
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Command palette"
        onClick={(e) => e.stopPropagation()}
        className="w-[640px] max-w-[92vw] overflow-hidden border border-[color:var(--border-strong)] bg-[color:var(--bg-elev)] shadow-[var(--shadow-pop)]"
      >
        <div className="flex items-center gap-[8px] border-b border-[color:var(--border)] px-[14px] py-[12px]">
          <Icon.Search
            size={13}
            strokeWidth={ICON_DEFAULTS.strokeWidth}
            className="text-[color:var(--text-muted)]"
          />
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Type a command, prompt, or CLI snippet…"
            className="flex-1 border-0 bg-transparent font-[family-name:var(--font-body)] text-[14px] text-[color:var(--text)] outline-0 placeholder:text-[color:var(--text-faint)]"
            data-testid="command-palette-input"
          />
          <KBD>esc</KBD>
        </div>

        <div className="max-h-[360px] overflow-auto p-[6px]">
          {filtered.length === 0 && (
            <div className="p-[24px] text-center font-[family-name:var(--font-mono)] text-[12px] text-[color:var(--text-faint)]">
              No matches
            </div>
          )}
          {filtered.map((c) => (
            <button
              key={c.id}
              type="button"
              onClick={c.run}
              className="flex w-full cursor-pointer items-center gap-[10px] border-0 bg-transparent px-[10px] py-[8px] text-left text-[color:var(--text)] hover:bg-[color:var(--bg-elev-2)]"
            >
              <span
                className={[
                  "inline-flex h-[22px] w-[22px] items-center justify-center border border-[color:var(--border)]",
                  c.kind === "cli"
                    ? "bg-[color:var(--bg-sunken)] text-[color:var(--text-muted)]"
                    : "bg-[color:var(--accent-soft)] text-[color:var(--accent)]",
                ].join(" ")}
              >
                {c.kind === "cli" ? (
                  <Icon.Terminal size={11} strokeWidth={ICON_DEFAULTS.strokeWidth} />
                ) : c.kind === "nav" ? (
                  <Icon.ArrowRight size={11} strokeWidth={ICON_DEFAULTS.strokeWidth} />
                ) : (
                  <Icon.Sparkle size={11} strokeWidth={ICON_DEFAULTS.strokeWidth} />
                )}
              </span>
              <span className="min-w-0 flex-1">
                <div className="text-[length:var(--font-size-sm)]">{c.label}</div>
                {c.cmd && (
                  <div className="mt-[1px] font-[family-name:var(--font-mono)] text-[11px] text-[color:var(--text-faint)]">
                    $ {c.cmd}
                  </div>
                )}
              </span>
              {c.shortcut && <KBD>{c.shortcut}</KBD>}
            </button>
          ))}
        </div>

        <div className="flex items-center gap-[8px] border-t border-[color:var(--border)] px-[12px] py-[6px] font-[family-name:var(--font-mono)] text-[11px] text-[color:var(--text-faint)]">
          <KBD>↑</KBD>
          <KBD>↓</KBD>
          navigate
          <span className="opacity-50">·</span>
          <KBD>↵</KBD>
          run
          <span className="opacity-50">·</span>
          <KBD>⌘C</KBD>
          copy CLI
        </div>
      </div>
    </div>
  );
}
