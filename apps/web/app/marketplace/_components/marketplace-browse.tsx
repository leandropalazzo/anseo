"use client";

import { useState } from "react";
import Link from "next/link";

import { Card } from "@/components/ui/card";
import { Pill } from "@/components/ui/pill";
import { VerifiedBadge } from "@/components/marketplace/verified-badge";
import { CapabilityBlock } from "@/components/marketplace/capability-block";
import type { MarketplacePlugin } from "@/lib/api";
import { stripEmoji } from "@/lib/plugin-format";

type Tab = "available" | "installed";

export function MarketplaceBrowse({
  plugins,
}: {
  plugins: MarketplacePlugin[];
}) {
  const [tab, setTab] = useState<Tab>("available");
  const installed = plugins.filter((p) => p.installed);
  const shown = tab === "installed" ? installed : plugins;

  return (
    <div className="flex flex-col gap-[12px]">
      <div
        role="tablist"
        aria-label="Marketplace tabs"
        data-testid="marketplace-tabs"
        className="flex gap-[4px]"
      >
        <TabButton
          id="available"
          active={tab === "available"}
          onClick={() => setTab("available")}
        >
          Available
        </TabButton>
        {/* UX-DR99 — Installed tab has parity with Available (same cards). */}
        <TabButton
          id="installed"
          active={tab === "installed"}
          onClick={() => setTab("installed")}
        >
          Installed ({installed.length})
        </TabButton>
      </div>

      {shown.length === 0 ? (
        <Card>
          {/* UX-DR98 — zero-state guidance, not an empty void. */}
          <div
            data-testid="marketplace-empty"
            className="flex flex-col gap-[4px] text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]"
          >
            <span>
              {tab === "installed"
                ? "No plugins installed yet."
                : "No plugins available in the registry."}
            </span>
            <span className="text-[color:var(--text-faint)]">
              Browse the registry or run{" "}
              <code className="font-[family-name:var(--font-mono)]">
                ogeo plugin install &lt;id&gt;@&lt;version&gt;
              </code>{" "}
              from the CLI.
            </span>
          </div>
        </Card>
      ) : (
        <ul
          data-testid="marketplace-list"
          className="m-0 grid list-none grid-cols-2 gap-[12px] p-0"
        >
          {shown.map((p) => (
            <li key={p.slug}>
              <Link
                href={`/marketplace/${encodeURIComponent(p.slug)}`}
                data-testid="plugin-card"
                data-slug={p.slug}
                className="block h-full"
              >
                <Card className="h-full hover:border-[color:var(--accent)]">
                  <div className="flex flex-col gap-[8px]">
                    <div className="flex items-center justify-between gap-[8px]">
                      <VerifiedBadge
                        verified={p.verified}
                        signature_status={p.signature_status}
                      />
                      {p.update_available && p.installed && (
                        <span data-testid="plugin-update-available">
                          <Pill mono tone="info">
                            update
                          </Pill>
                        </span>
                      )}
                    </div>
                    <div className="font-medium text-[color:var(--text)]">
                      {stripEmoji(p.name)}
                    </div>
                    <div className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
                      {p.slug} · v{p.version} · {p.plugin_type}
                    </div>
                    <p className="m-0 text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
                      {p.description}
                    </p>
                    <CapabilityBlock capabilities={p.capabilities} />
                  </div>
                </Card>
              </Link>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function TabButton({
  id,
  active,
  onClick,
  children,
}: {
  id: string;
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      role="tab"
      aria-selected={active}
      data-testid={`marketplace-tab-${id}`}
      onClick={onClick}
      className={[
        "border px-[10px] py-[4px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)]",
        active
          ? "border-[color:var(--accent)] text-[color:var(--text)]"
          : "border-[color:var(--border)] text-[color:var(--text-muted)] hover:text-[color:var(--text)]",
      ].join(" ")}
    >
      {children}
    </button>
  );
}
