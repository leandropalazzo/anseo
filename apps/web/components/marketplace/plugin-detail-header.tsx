import { Card } from "@/components/ui/card";
import { Pill } from "@/components/ui/pill";
import type { MarketplacePlugin } from "@/lib/api";
import { stripEmoji } from "@/lib/plugin-format";

import { VerifiedBadge } from "./verified-badge";

// UX-DR96 — the plugin detail header composes on top of card.tsx. It pins the
// trust badge, the explicit pinned version (UX-DR93, never `latest`), the
// plugin type, and the emoji-stripped name (UX-DR97).
export function PluginDetailHeader({ plugin }: { plugin: MarketplacePlugin }) {
  return (
    <Card accent>
      <div
        data-testid="plugin-detail-header"
        className="flex flex-col gap-[8px]"
      >
        <div className="flex items-center gap-[8px]">
          <VerifiedBadge
            verified={plugin.verified}
            signature_status={plugin.signature_status}
          />
          <Pill mono>{plugin.plugin_type}</Pill>
          {plugin.update_available && plugin.installed && (
            <span data-testid="plugin-update-available">
              <Pill mono tone="info">
                update available
              </Pill>
            </span>
          )}
        </div>
        <h1
          data-testid="plugin-name"
          className="m-0 text-[length:20px] font-normal tracking-[var(--display-tracking)] text-[color:var(--text)]"
        >
          {stripEmoji(plugin.name)}
        </h1>
        <div className="flex flex-wrap items-center gap-[6px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
          <span data-testid="plugin-slug">{plugin.slug}</span>
          <span>·</span>
          {/* UX-DR93 — version is always explicit + pinned. */}
          <span data-testid="plugin-version">v{plugin.version}</span>
          <span>·</span>
          <span>by {plugin.author}</span>
        </div>
        <p className="m-0 max-w-[70ch] text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
          {plugin.description}
        </p>
      </div>
    </Card>
  );
}
