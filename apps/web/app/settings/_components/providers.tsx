"use client";

import { useEffect, useState } from "react";
import { Card } from "@/components/ui/card";
import { Pill } from "@/components/ui/pill";
import { ProviderDot } from "@/components/ui/provider-dot";
import {
  CONCRETE_PROVIDER_IDS,
  configuredConcreteProviderIds,
  resolveProviderIdentity,
} from "@/lib/provider-colors";
import type { SetupStatus } from "@/lib/api";
import { ApiKeysCard } from "@/app/setup/_components/api-keys-card";

// Default model per provider — mirrors `ProviderName::default_model()` in
// crates/core (config.rs). Shown read-only so the operator sees which model a
// connected provider runs without us fabricating per-provider metadata.
const DEFAULT_MODEL: Record<string, string> = {
  openai: "gpt-4o-2024-08-06",
  anthropic: "claude-3-5-sonnet-20241022",
  gemini: "gemini-1.5-pro",
  perplexity: "sonar-pro",
  grok: "grok-2-latest",
  mistral: "mistral-large-latest",
};

export function ProvidersSection() {
  const [apiKeys, setApiKeys] = useState<SetupStatus["api_keys"] | null>(null);

  useEffect(() => {
    let cancelled = false;
    fetch("/api/setup/status", { cache: "no-store" })
      .then((r) => (r.ok ? r.json() : Promise.reject(r.status)))
      .then((s: SetupStatus) => {
        if (!cancelled) setApiKeys(s.api_keys);
      })
      .catch(() => {
        if (!cancelled) setApiKeys([]);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <div className="flex flex-col gap-[16px]" data-testid="settings-providers">
      {apiKeys !== null && <ApiKeysCard api_keys={apiKeys} />}
      {apiKeys !== null && <ProviderOverview apiKeys={apiKeys} />}
    </div>
  );
}

function ProviderOverview({ apiKeys }: { apiKeys: SetupStatus["api_keys"] }) {
  const concreteConfigured = new Set(configuredConcreteProviderIds(apiKeys));
  const directConfigured = new Set(
    apiKeys.filter((k) => k.configured).map((k) => k.provider.toLowerCase()),
  );
  const openRouterConfigured = directConfigured.has("openrouter");

  return (
    <Card eyebrow="configured models" title="Providers">
      {concreteConfigured.size === 0 && (
        <p className="m-0 text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
          No providers configured yet. Add a key above to connect one.
        </p>
      )}
      {CONCRETE_PROVIDER_IDS.map((provider, i) => {
        const identity = resolveProviderIdentity(provider);
        const model = DEFAULT_MODEL[provider] ?? "default";
        const isDirect = directConfigured.has(provider);
        const isConnected = concreteConfigured.has(provider);
        const source = isDirect
          ? "direct key"
          : openRouterConfigured
            ? "OpenRouter fallback"
            : null;
        return (
          <div
            key={provider}
            className="flex flex-wrap items-center gap-[12px] py-[12px]"
            style={{
              borderBottom:
                i === CONCRETE_PROVIDER_IDS.length - 1
                  ? "0"
                  : "1px solid var(--hairline)",
            }}
            data-testid={`provider-row-${provider}`}
          >
            <div className="flex min-w-[130px] items-center gap-[8px] whitespace-nowrap">
              <ProviderDot provider={provider} size={14} />
              <span className="text-[length:var(--font-size-sm)] text-[color:var(--text)]">
                {identity.label}
              </span>
            </div>
            <div className="min-w-0 flex-1 basis-[220px]">
              <div className="overflow-hidden text-ellipsis whitespace-nowrap font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text)]">
                {isConnected ? model : "—"}
              </div>
              {source && (
                <div className="mt-[2px] overflow-hidden text-ellipsis whitespace-nowrap font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
                  {source}
                </div>
              )}
            </div>
            <Pill mono tone={isConnected ? "ok" : "neutral"}>
              <span
                className="mr-[4px] inline-block h-[6px] w-[6px] rounded-full"
                style={{
                  background: isConnected ? "var(--ok)" : "var(--text-faint)",
                }}
              />
              {isConnected ? "connected" : "disconnected"}
            </Pill>
          </div>
        );
      })}
    </Card>
  );
}
