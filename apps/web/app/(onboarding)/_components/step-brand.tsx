"use client";

import { ArrowRight, Sparkles } from "lucide-react";
import { useEffect, useState, useTransition } from "react";

import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Pill } from "@/components/ui/pill";
import { ICON_DEFAULTS } from "@/lib/icons";
import {
  DEFAULT_BRAND,
  DEFAULT_BRAND_ALIASES,
  DEFAULT_COMPETITORS,
} from "@/lib/mock-ops";
import {
  getBrand,
  putBrand,
  suggestCompetitors,
  type SetupStatus,
} from "@/lib/api";
import {
  configuredConcreteProviderIds,
  resolveProviderIdentity,
} from "@/lib/provider-colors";

export interface StepBrandProps {
  onNext: () => void;
}

const INPUT_CLASS =
  "mt-[6px] w-full border border-[color:var(--border)] bg-[color:var(--bg-sunken)] px-[10px] py-[8px] text-[length:var(--font-size-sm)] text-[color:var(--text)] outline-0 font-[family-name:var(--font-mono)]";

function splitAliases(s: string): string[] {
  return s
    .split(",")
    .map((x) => x.trim())
    .filter(Boolean);
}

export function StepBrand({ onNext }: StepBrandProps) {
  const [name, setName] = useState(DEFAULT_BRAND);
  const [aliases, setAliases] = useState(DEFAULT_BRAND_ALIASES);
  const [competitors, setCompetitors] = useState<string[]>([
    ...DEFAULT_COMPETITORS,
  ]);
  const [draftCompetitor, setDraftCompetitor] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [saving, startSaving] = useTransition();

  const [providers, setProviders] = useState<string[]>([]);
  const [provider, setProvider] = useState("");
  const [suggesting, setSuggesting] = useState(false);
  const [suggestError, setSuggestError] = useState<string | null>(null);

  // Hydrate from the DB-authoritative brand config; fall back to the seeded
  // defaults when the project has not been saved yet.
  useEffect(() => {
    let cancelled = false;
    getBrand()
      .then((b) => {
        if (cancelled) return;
        setName(b.name);
        setAliases(b.variants.join(", "));
        if (b.competitors.length > 0) {
          setCompetitors(b.competitors.map((c) => c.name));
        }
      })
      .catch(() => {
        /* keep seeded defaults */
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    fetch("/api/setup/status", { cache: "no-store" })
      .then((r) => (r.ok ? r.json() : Promise.reject(r.status)))
      .then((s: SetupStatus) => {
        if (cancelled) return;
        const configured = configuredConcreteProviderIds(s.api_keys);
        setProviders(configured);
        if (configured[0]) setProvider(configured[0]);
      })
      .catch(() => {
        if (!cancelled) setProviders([]);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  function handleSuggest() {
    if (!provider) return;
    setSuggestError(null);
    setSuggesting(true);
    suggestCompetitors(provider)
      .then((res) => {
        if (res.error) {
          setSuggestError(res.message ?? res.error);
          return;
        }
        setCompetitors((prev) => {
          const seen = new Set(prev.map((x) => x.toLowerCase()));
          const next = [...prev];
          for (const c of res.competitors) {
            const key = c.name.trim().toLowerCase();
            if (key && !seen.has(key)) {
              seen.add(key);
              next.push(c.name.trim());
            }
          }
          return next;
        });
      })
      .catch((e) => setSuggestError(String(e)))
      .finally(() => setSuggesting(false));
  }

  function removeCompetitor(c: string) {
    setCompetitors((prev) => prev.filter((x) => x !== c));
  }

  function addCompetitor() {
    const v = draftCompetitor.trim();
    if (v && !competitors.includes(v)) {
      setCompetitors((prev) => [...prev, v]);
    }
    setDraftCompetitor("");
  }

  function handleContinue() {
    if (!name.trim()) {
      setError("Brand name must not be empty.");
      return;
    }
    setError(null);
    startSaving(async () => {
      const res = await putBrand({
        name: name.trim(),
        variants: splitAliases(aliases),
        competitors: competitors.map((c) => ({ name: c, variants: [] })),
      });
      if (res.error) {
        setError(res.message ?? res.error);
        return;
      }
      onNext();
    });
  }

  return (
    <Card eyebrow="step 3 · brand & competitors" title="What are we tracking?">
      <div className="grid grid-cols-2 gap-[12px]">
        <div>
          <div className="label-eyebrow text-[color:var(--text-faint)]">
            your brand
          </div>
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            data-testid="onboarding-brand-name"
            className={INPUT_CLASS}
          />
          <div className="mt-[12px] label-eyebrow text-[color:var(--text-faint)]">
            aliases (comma-separated)
          </div>
          <input
            value={aliases}
            onChange={(e) => setAliases(e.target.value)}
            data-testid="onboarding-brand-aliases"
            className={INPUT_CLASS}
          />
        </div>
        <div>
          <div className="label-eyebrow text-[color:var(--text-faint)]">
            competitors
          </div>
          <div className="mt-[6px] flex flex-wrap gap-[6px] border border-[color:var(--border)] bg-[color:var(--bg-sunken)] p-[10px]">
            {competitors.map((c) => (
              <Pill key={c} tone="accent" mono>
                {c}{" "}
                <button
                  type="button"
                  onClick={() => removeCompetitor(c)}
                  aria-label={`Remove ${c}`}
                  className="ml-[4px] cursor-pointer appearance-none border-0 bg-transparent p-0 text-[color:inherit] opacity-60 hover:opacity-100"
                >
                  ×
                </button>
              </Pill>
            ))}
            <input
              value={draftCompetitor}
              onChange={(e) => setDraftCompetitor(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  addCompetitor();
                }
              }}
              onBlur={addCompetitor}
              placeholder="+ add competitor"
              data-testid="onboarding-competitor-input"
              className="min-w-[120px] flex-1 border-0 bg-transparent font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-muted)] outline-0"
            />
          </div>
          <div className="mt-[8px] flex flex-wrap items-center gap-[8px]">
            {providers.length > 0 ? (
              <>
                <select
                  data-testid="onboarding-suggest-provider"
                  value={provider}
                  onChange={(e) => setProvider(e.target.value)}
                  className="border border-[color:var(--border)] bg-[color:var(--bg-sunken)] px-[8px] py-[6px] text-[length:var(--font-size-xs)] text-[color:var(--text)] outline-0"
                >
                    {providers.map((p) => (
                      <option key={p} value={p}>
                        {resolveProviderIdentity(p).label}
                      </option>
                    ))}
                </select>
                <Button
                  size="sm"
                  variant="ghost"
                  disabled={suggesting}
                  onClick={handleSuggest}
                  data-testid="onboarding-suggest-competitors"
                  leadingIcon={
                    <Sparkles
                      size={11}
                      strokeWidth={ICON_DEFAULTS.strokeWidth}
                    />
                  }
                >
                  {suggesting ? "Suggesting…" : "Suggest competitors via AI"}
                </Button>
              </>
            ) : (
              <span className="text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
                Add a provider key to enable AI suggestions.
              </span>
            )}
          </div>
          {suggestError && (
            <p
              role="alert"
              data-testid="onboarding-suggest-error"
              className="mt-[6px] text-[length:var(--font-size-xs)] text-[color:var(--danger)]"
            >
              {suggestError}
            </p>
          )}
        </div>
      </div>
      {error && (
        <p
          role="alert"
          data-testid="onboarding-brand-error"
          className="mt-[10px] text-[length:var(--font-size-xs)] text-[color:var(--danger)]"
        >
          {error}
        </p>
      )}
      <div className="mt-[16px] flex justify-end">
        <Button
          variant="primary"
          size="sm"
          disabled={saving}
          onClick={handleContinue}
          data-testid="onboarding-brand-continue"
          leadingIcon={
            <ArrowRight size={11} strokeWidth={ICON_DEFAULTS.strokeWidth} />
          }
        >
          {saving ? "Saving…" : "Continue"}
        </Button>
      </div>
    </Card>
  );
}
