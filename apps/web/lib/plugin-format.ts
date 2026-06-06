// Story 17.7 — display helpers for the marketplace surface.
//
// UX-DR97 — plugin names must not render raw emoji in the brutalist Signal
// surface; every emoji/pictographic codepoint collapses to a single `▒` glyph
// so a name like "Demo Plugin 🚀" renders as "Demo Plugin ▒".

import type { PluginCapability } from "./api";

const EMOJI_RE =
  /(\p{Extended_Pictographic}|\p{Emoji_Presentation})(‍(\p{Extended_Pictographic}|\p{Emoji_Presentation})|️)*/gu;

export function stripEmoji(label: string): string {
  return label.replace(EMOJI_RE, "▒").replace(/\s+/g, " ").trim();
}

// Human-readable, deterministic label for a capability — used by the
// always-visible capability disclosure block (UX-DR94).
export function capabilityLabel(cap: PluginCapability): string {
  switch (cap.kind) {
    case "network":
      return `network → ${cap.allowlist.join(", ")}`;
    case "read-secret":
      return `read-secret → ${cap.keys.join(", ")}`;
    case "emit-event":
      return `emit-event → ${cap.kinds.join(", ")}`;
    case "extractor-confidence-override":
      return "extractor-confidence-override";
    case "analytics-window":
      return `analytics-window → ${cap.windows.join(", ")}`;
  }
}
