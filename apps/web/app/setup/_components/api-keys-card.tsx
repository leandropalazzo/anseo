"use client";

import { useState, useTransition } from "react";
import { Card } from "@/components/ui/card";
import { Pill } from "@/components/ui/pill";
import { Button } from "@/components/ui/button";
import { ProviderDot } from "@/components/ui/provider-dot";
import { postApiKeyRevoke, postApiKeySet } from "@/lib/api";
import type { SetupStatus } from "@/lib/api";
import { resolveProviderIdentity } from "@/lib/provider-colors";

interface Props {
  api_keys: SetupStatus["api_keys"];
}

export function ApiKeysCard({ api_keys: initialKeys }: Props) {
  const [keys, setKeys] = useState(initialKeys);
  const [revoking, setRevoking] = useState<string | null>(null);
  const [editing, setEditing] = useState<string | null>(null);
  const [draftKey, setDraftKey] = useState("");
  const [saving, setSaving] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [, startTransition] = useTransition();

  function handleRevoke(provider: string) {
    setRevoking(provider);
    startTransition(async () => {
      await postApiKeyRevoke(provider);
      // Optimistically mark as not configured after revoke
      setKeys((prev) =>
        prev.map((k) =>
          k.provider === provider ? { ...k, configured: false, last_used_at: null } : k,
        ),
      );
      setRevoking(null);
    });
  }

  function keyLabel(provider: string): string {
    return provider === "openrouter"
      ? "OpenRouter key"
      : resolveProviderIdentity(provider).label;
  }

  function openEditor(provider: string) {
    setEditing(provider);
    setDraftKey("");
    setError(null);
  }

  function handleSave(provider: string) {
    if (!draftKey.trim()) {
      setError("Enter a key before saving.");
      return;
    }
    setSaving(provider);
    setError(null);
    startTransition(async () => {
      const res = await postApiKeySet(provider, draftKey.trim());
      setSaving(null);
      if (res.configured) {
        setKeys((prev) =>
          prev.map((k) =>
            k.provider === provider ? { ...k, configured: true } : k,
          ),
        );
        setEditing(null);
        setDraftKey("");
      } else {
        setError(res.error ?? res.message ?? "Failed to store key.");
      }
    });
  }

  return (
    <Card eyebrow="credentials" title="API Keys">
      <div data-testid="api-keys-table" className="flex flex-col gap-0">
        {keys.length === 0 ? (
          <p className="m-0 text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
            No API keys configured.
          </p>
        ) : (
          <>
            {/* Table header */}
            <div
              className="grid items-center gap-[8px] pb-[6px]"
              style={{
                gridTemplateColumns: "1fr auto auto auto",
                borderBottom: "1px solid var(--hairline)",
              }}
            >
              <span className="text-[length:var(--font-size-xs)] text-[color:var(--text-faint)] uppercase tracking-[0.05em]">
                Provider
              </span>
              <span className="text-[length:var(--font-size-xs)] text-[color:var(--text-faint)] uppercase tracking-[0.05em]">
                Status
              </span>
              <span className="text-[length:var(--font-size-xs)] text-[color:var(--text-faint)] uppercase tracking-[0.05em]">
                Last Used
              </span>
              <span className="sr-only">Action</span>
            </div>

            {/* Rows */}
            {keys.map((key) => (
              <div
                key={key.provider}
                data-testid={`api-key-row-${key.provider}`}
                style={{ borderBottom: "1px solid var(--hairline)" }}
              >
                <div
                  className="grid items-center gap-[8px] py-[8px]"
                  style={{ gridTemplateColumns: "1fr auto auto auto auto" }}
                >
                  <span className="inline-flex min-w-0 items-center gap-[8px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-sm)] text-[color:var(--text)]">
                    <ProviderDot provider={key.provider} size={14} />
                    <span className="truncate">{keyLabel(key.provider)}</span>
                  </span>

                  <Pill tone={key.configured ? "ok" : "neutral"}>
                    <span
                      aria-hidden
                      className="mr-[4px] inline-block h-[6px] w-[6px] rounded-full"
                      style={{
                        background: key.configured ? "var(--ok)" : "var(--text-faint)",
                      }}
                    />
                    {key.configured ? "configured" : "not set"}
                  </Pill>

                  <span className="text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
                    {key.last_used_at
                      ? new Date(key.last_used_at).toLocaleDateString()
                      : "—"}
                  </span>

                  <Button
                    data-testid={`api-key-set-${key.provider}`}
                    variant="ghost"
                    size="sm"
                    disabled={saving === key.provider}
                    onClick={() => openEditor(key.provider)}
                    aria-label={`${key.configured ? "Update" : "Set"} API key for ${keyLabel(key.provider)}`}
                  >
                    {key.configured ? "Update" : "Set key"}
                  </Button>

                  <Button
                    data-testid={`api-key-revoke-${key.provider}`}
                    variant="ghost"
                    size="sm"
                    disabled={!key.configured || revoking === key.provider}
                    onClick={() => handleRevoke(key.provider)}
                    aria-label={`Revoke API key for ${keyLabel(key.provider)}`}
                  >
                    {revoking === key.provider ? "Revoking…" : "Revoke"}
                  </Button>
                </div>

                {editing === key.provider && (
                  <div
                    data-testid={`api-key-editor-${key.provider}`}
                    className="flex flex-col gap-[6px] pb-[10px]"
                  >
                    <div className="flex items-center gap-[8px]">
                      <input
                        type="password"
                        data-testid={`api-key-input-${key.provider}`}
                        value={draftKey}
                        onChange={(e) => setDraftKey(e.target.value)}
                        placeholder={`Paste ${keyLabel(key.provider)}`}
                        aria-label={`API key for ${keyLabel(key.provider)}`}
                        autoComplete="off"
                        className="flex-1 rounded-[6px] border border-[color:var(--hairline)] bg-[color:var(--surface)] px-[10px] py-[6px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-sm)] text-[color:var(--text)]"
                      />
                      <Button
                        data-testid={`api-key-save-${key.provider}`}
                        variant="primary"
                        size="sm"
                        disabled={saving === key.provider}
                        onClick={() => handleSave(key.provider)}
                      >
                        {saving === key.provider ? "Saving…" : "Save"}
                      </Button>
                      <Button
                        variant="ghost"
                        size="sm"
                        disabled={saving === key.provider}
                        onClick={() => {
                          setEditing(null);
                          setDraftKey("");
                          setError(null);
                        }}
                      >
                        Cancel
                      </Button>
                    </div>
                    {error && (
                      <p
                        role="alert"
                        data-testid={`api-key-error-${key.provider}`}
                        className="m-0 text-[length:var(--font-size-xs)] text-[color:var(--danger)]"
                      >
                        {error}
                      </p>
                    )}
                  </div>
                )}
              </div>
            ))}
          </>
        )}

        <p className="m-0 mt-[8px] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
          Manage API keys in Settings → Providers
        </p>
      </div>
    </Card>
  );
}
