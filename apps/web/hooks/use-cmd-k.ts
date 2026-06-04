"use client";

import { useCallback, useEffect, useState } from "react";

/**
 * ⌘K / Ctrl-K global keybinding for the Command Palette.
 *
 * - Meta+K on macOS, Ctrl+K elsewhere — both wired here.
 * - Esc closes when open.
 * - Ignores the keystroke while the user is typing in an input/textarea
 *   *unless* they hit the Cmd/Ctrl modifier (palette should still open
 *   over a focused text input).
 */
export interface UseCmdK {
  open: boolean;
  setOpen: (open: boolean) => void;
  toggle: () => void;
}

export function useCmdK(): UseCmdK {
  const [open, setOpen] = useState(false);

  const toggle = useCallback(() => setOpen((v) => !v), []);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const isK = e.key === "k" || e.key === "K";
      if (isK && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        setOpen((v) => !v);
        return;
      }
      if (e.key === "Escape") {
        setOpen(false);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  return { open, setOpen, toggle };
}
