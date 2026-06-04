"use client";

import { useEffect, useState } from "react";
import { ArrowRight, Check } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Pill } from "@/components/ui/pill";
import { ProviderDot } from "@/components/ui/provider-dot";
import {
  fetchSetupStatus,
  postApiKeySet,
  type SetupStatus,
} from "@/lib/api/setup";
import { ICON_DEFAULTS } from "@/lib/icons";
import {
  PROVIDER_COLORS,
  configuredConcreteProviderIds,
  type ProviderId,
} from "@/lib/provider-colors";

// Credentials the operator can store during onboarding. OpenRouter is listed as
// a key route; provider analytics and schedules still use concrete providers.
const PROVIDER_KEYS: ReadonlyArray<{
  id: ProviderId;
  model: string;
  note?: string;
}> = [
  { id: "openai", model: "gpt-4o-2024-08-06" },
  { id: "anthropic", model: "claude-3-5-sonnet-20241022" },
  { id: "gemini", model: "gemini-1.5-pro" },
  { id: "perplexity", model: "sonar-pro" },
  { id: "grok", model: "grok-2-latest" },
  { id: "mistral", model: "mistral-large-latest" },
  {
    id: "openrouter",
    model: "openrouter/auto",
    note: "One key routes to OpenAI, Anthropic, Gemini, and more.",
  },
];

export interface StepConnectProvidersProps {
  onNext: () => void;
}

export function StepConnectProviders({ onNext }: StepConnectProvidersProps) {
  const [connected, setConnected] = useState<Set<ProviderId>>(new Set());
  const [editing, setEditing] = useState<ProviderId | null>(null);
  const [draftKey, setDraftKey] = useState("");
  const [saving, setSaving] = useState<ProviderId | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Seed connection state from live setup status so providers already keyed
  // (e.g. via the CLI or settings) show as connected.
  useEffect(() => {
    let cancelled = false;
    fetchSetupStatus()
      .then((s: SetupStatus) => {
        if (cancelled) return;
        const next = new Set<ProviderId>();
        for (const k of s.api_keys) {
          if (k.configured && k.provider in PROVIDER_COLORS) {
            next.add(k.provider as ProviderId);
          }
        }
        setConnected(next);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  const concreteConnectedCount = configuredConcreteProviderIds(
    [...connected].map((provider) => ({ provider, configured: true })),
  ).length;

  const startEdit = (id: ProviderId) => {
    setEditing(id);
    setDraftKey("");
    setError(null);
  };

  const save = async (id: ProviderId) => {
    if (!draftKey.trim()) {
      setError("Paste a key first.");
      return;
    }
    setSaving(id);
    setError(null);
    const res = await postApiKeySet(id, draftKey.trim());
    setSaving(null);
    if (res.configured) {
      setConnected((prev) => new Set(prev).add(id));
      setEditing(null);
      setDraftKey("");
    } else {
      setError(res.message ?? res.error ?? "Failed to store key.");
    }
  };

  return (
    <Card
      eyebrow="step 2 · access"
      title="Connect provider access"
      action={
        <Pill mono>
          {concreteConnectedCount} providers ready
        </Pill>
      }
    >
      <div className="flex flex-col gap-[8px]">
        {PROVIDER_KEYS.map(({ id, model, note }) => {
          const isConnected = connected.has(id);
          const isEditing = editing === id;
          return (
            <div
              key={id}
              className="flex flex-col gap-[8px] border border-[color:var(--border)] px-[10px] py-[8px]"
              style={{
                background: isConnected
                  ? "color-mix(in oklch, var(--ok) 4%, var(--bg-elev))"
                  : "var(--bg-elev)",
              }}
              data-testid={`connect-${id}`}
            >
              <div className="grid items-center gap-[12px] [grid-template-columns:30px_1fr_100px]">
                <ProviderDot provider={id} size={14} />
                <div className="min-w-0">
                  <div className="text-[length:var(--font-size-sm)] text-[color:var(--text)]">
                    {id === "openrouter"
                      ? "OpenRouter key"
                      : PROVIDER_COLORS[id].label}
                  </div>
                  <div className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
                    {model}
                  </div>
                  {note && (
                    <div className="mt-[2px] text-[length:var(--font-size-xs)] text-[color:var(--text-muted)]">
                      {note}
                    </div>
                  )}
                </div>
                <Button
                  size="sm"
                  variant={isConnected ? "ghost" : "primary"}
                  onClick={() =>
                    isConnected ? undefined : isEditing ? setEditing(null) : startEdit(id)
                  }
                  disabled={isConnected}
                  leadingIcon={
                    isConnected ? (
                      <Check
                        size={11}
                        strokeWidth={ICON_DEFAULTS.strokeWidth}
                        color="var(--ok)"
                      />
                    ) : undefined
                  }
                >
                  {isConnected ? "Connected" : isEditing ? "Cancel" : "Connect"}
                </Button>
              </div>

              {isEditing && !isConnected && (
                <div className="flex flex-col gap-[6px]">
                  <div className="flex items-center gap-[8px]">
                    <input
                      type="password"
                      data-testid={`connect-input-${id}`}
                      value={draftKey}
                      onChange={(e) => setDraftKey(e.target.value)}
                      placeholder={`Paste ${PROVIDER_COLORS[id].label} API key`}
                      aria-label={`API key for ${id}`}
                      autoComplete="off"
                      className="flex-1 border border-[color:var(--hairline)] bg-[color:var(--bg-sunken)] px-[10px] py-[6px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-sm)] text-[color:var(--text)]"
                    />
                    <Button
                      data-testid={`connect-save-${id}`}
                      size="sm"
                      variant="primary"
                      disabled={saving === id}
                      onClick={() => void save(id)}
                    >
                      {saving === id ? "Saving…" : "Save"}
                    </Button>
                  </div>
                  {error && editing === id && (
                    <p
                      role="alert"
                      data-testid={`connect-error-${id}`}
                      className="m-0 text-[length:var(--font-size-xs)] text-[color:var(--danger)]"
                    >
                      {error}
                    </p>
                  )}
                </div>
              )}
            </div>
          );
        })}
      </div>
      <div className="mt-[16px] flex items-center justify-between">
        <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
          keys stored in OS keychain. Never transmitted.
        </span>
        <Button
          variant="primary"
          size="sm"
          onClick={onNext}
          disabled={concreteConnectedCount === 0}
          leadingIcon={
            <ArrowRight size={11} strokeWidth={ICON_DEFAULTS.strokeWidth} />
          }
        >
          Continue
        </Button>
      </div>
    </Card>
  );
}
