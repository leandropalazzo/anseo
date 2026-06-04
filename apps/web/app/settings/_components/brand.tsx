"use client";

import { useEffect, useState, useTransition } from "react";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Pill } from "@/components/ui/pill";
import {
  getBrand,
  putBrand,
  suggestCompetitors,
  type BrandView,
  type CompetitorConfig,
  type SetupStatus,
} from "@/lib/api";
import {
  configuredConcreteProviderIds,
  resolveProviderIdentity,
} from "@/lib/provider-colors";

const inputClass =
  "flex-1 rounded-[6px] border border-[color:var(--hairline)] bg-[color:var(--surface)] px-[10px] py-[6px] text-[length:var(--font-size-sm)] text-[color:var(--text)]";

function linesToList(s: string): string[] {
  return s
    .split(/[\n,]/)
    .map((x) => x.trim())
    .filter(Boolean);
}

/** Merge AI-suggested names into the existing competitor list, deduped
 *  case-insensitively by name. Existing entries (and their variants) win. */
function mergeCompetitors(
  existing: CompetitorConfig[],
  suggested: CompetitorConfig[],
): CompetitorConfig[] {
  const seen = new Set(existing.map((c) => c.name.trim().toLowerCase()));
  const merged = [...existing];
  for (const s of suggested) {
    const key = s.name.trim().toLowerCase();
    if (!key || seen.has(key)) continue;
    seen.add(key);
    merged.push({ name: s.name.trim(), variants: s.variants ?? [] });
  }
  return merged;
}

export function BrandSection() {
  const [brand, setBrand] = useState<BrandView | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);

  const [name, setName] = useState("");
  const [variants, setVariants] = useState("");
  const [siteUrl, setSiteUrl] = useState("");
  const [competitors, setCompetitors] = useState<CompetitorConfig[]>([]);

  const [saving, startSaving] = useTransition();
  const [error, setError] = useState<string | null>(null);
  const [restartRequired, setRestartRequired] = useState(false);
  const [saved, setSaved] = useState(false);

  const [providers, setProviders] = useState<string[]>([]);
  const [provider, setProvider] = useState("");
  const [suggesting, setSuggesting] = useState(false);
  const [suggestError, setSuggestError] = useState<string | null>(null);

  function hydrate(b: BrandView) {
    setBrand(b);
    setName(b.name);
    setVariants(b.variants.join("\n"));
    setSiteUrl(b.site_url ?? "");
    setCompetitors(b.competitors);
  }

  useEffect(() => {
    let cancelled = false;
    getBrand()
      .then((b) => {
        if (!cancelled) hydrate(b);
      })
      .catch((e) => {
        if (!cancelled) setLoadError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // Configured providers drive the suggest-competitors picker; only providers
  // with a stored key can answer, so we list those.
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
        setCompetitors((prev) => mergeCompetitors(prev, res.competitors));
      })
      .catch((e) => setSuggestError(String(e)))
      .finally(() => setSuggesting(false));
  }

  const renamed = brand !== null && name.trim() !== brand.name;

  function handleSave() {
    const trimmed = name.trim();
    if (!trimmed) {
      setError("Brand name must not be empty.");
      return;
    }
    setError(null);
    setSaved(false);
    setRestartRequired(false);
    startSaving(async () => {
      const res = await putBrand({
        name: trimmed,
        variants: linesToList(variants),
        site_url: siteUrl.trim() || undefined,
        competitors: competitors
          .map((c) => ({ name: c.name.trim(), variants: c.variants.filter(Boolean) }))
          .filter((c) => c.name),
      });
      if (res.error) {
        setError(res.message ?? res.error);
        return;
      }
      hydrate(res);
      setSaved(true);
      setRestartRequired(res.restart_required);
    });
  }

  function addCompetitor() {
    setCompetitors((prev) => [...prev, { name: "", variants: [] }]);
  }

  function updateCompetitor(i: number, patch: Partial<CompetitorConfig>) {
    setCompetitors((prev) =>
      prev.map((c, idx) => (idx === i ? { ...c, ...patch } : c)),
    );
  }

  function removeCompetitor(i: number) {
    setCompetitors((prev) => prev.filter((_, idx) => idx !== i));
  }

  if (loadError) {
    return (
      <Card eyebrow="brand identity" title="Brand">
        <p role="alert" className="m-0 text-[length:var(--font-size-sm)] text-[color:var(--danger)]">
          Failed to load brand config: {loadError}
        </p>
      </Card>
    );
  }

  if (!brand) {
    return (
      <Card eyebrow="brand identity" title="Brand">
        <p className="m-0 text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
          Loading…
        </p>
      </Card>
    );
  }

  return (
    <div className="flex flex-col gap-[16px]" data-testid="settings-brand">
      <Card eyebrow="brand identity" title="Brand">
        <div className="flex flex-col gap-[14px]">
          <label className="flex flex-col gap-[6px]">
            <span className="text-[length:var(--font-size-sm)] text-[color:var(--text)]">
              Brand name
            </span>
            <input
              data-testid="brand-name-input"
              value={name}
              onChange={(e) => setName(e.target.value)}
              className={inputClass}
              autoComplete="off"
            />
            <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
              project_id: {brand.project_id}
            </span>
          </label>

          {renamed && (
            <p
              role="status"
              data-testid="brand-rename-warning"
              className="m-0 text-[length:var(--font-size-xs)] text-[color:var(--warn)]"
            >
              Changing the name re-derives the project identity. This is only
              allowed before the first run, and the API must be restarted to
              take effect.
            </p>
          )}

          <label className="flex flex-col gap-[6px]">
            <span className="text-[length:var(--font-size-sm)] text-[color:var(--text)]">
              Variants / aliases
            </span>
            <textarea
              data-testid="brand-variants-input"
              value={variants}
              onChange={(e) => setVariants(e.target.value)}
              rows={3}
              placeholder="One per line (or comma-separated)"
              className={`${inputClass} resize-y font-[family-name:var(--font-mono)]`}
            />
          </label>

          <label className="flex flex-col gap-[6px]">
            <span className="text-[length:var(--font-size-sm)] text-[color:var(--text)]">
              Website URL
            </span>
            <input
              data-testid="brand-site-url-input"
              value={siteUrl}
              onChange={(e) => setSiteUrl(e.target.value)}
              placeholder="https://example.com"
              autoComplete="off"
              className={inputClass}
            />
            <span className="text-[length:var(--font-size-xs)] text-[color:var(--text-muted)]">
              Your owned site. Prefills Site Audit and frames crawler observability.
            </span>
          </label>
        </div>
      </Card>

      <Card eyebrow="tracked competitors" title="Competitors">
        <div className="flex flex-col gap-[10px]" data-testid="brand-competitors">
          {competitors.length === 0 && (
            <p className="m-0 text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
              No competitors tracked yet.
            </p>
          )}
          {competitors.map((c, i) => (
            <div
              key={i}
              className="flex flex-col gap-[6px] border-b border-[color:var(--hairline)] pb-[10px]"
            >
              <div className="flex items-center gap-[8px]">
                <input
                  data-testid={`competitor-name-${i}`}
                  value={c.name}
                  onChange={(e) => updateCompetitor(i, { name: e.target.value })}
                  placeholder="Competitor name"
                  className={inputClass}
                  autoComplete="off"
                />
                <Button variant="ghost" size="sm" onClick={() => removeCompetitor(i)}>
                  Remove
                </Button>
              </div>
              <input
                data-testid={`competitor-variants-${i}`}
                value={c.variants.join(", ")}
                onChange={(e) =>
                  updateCompetitor(i, { variants: linesToList(e.target.value) })
                }
                placeholder="Variants (comma-separated)"
                className={`${inputClass} font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)]`}
                autoComplete="off"
              />
            </div>
          ))}
          <div className="flex flex-wrap items-center gap-[10px]">
            <Button variant="secondary" size="sm" onClick={addCompetitor} data-testid="competitor-add">
              Add competitor
            </Button>
            {providers.length > 0 ? (
              <>
                <select
                  data-testid="suggest-provider"
                  value={provider}
                  onChange={(e) => setProvider(e.target.value)}
                  className="rounded-[6px] border border-[color:var(--hairline)] bg-[color:var(--surface)] px-[8px] py-[6px] text-[length:var(--font-size-sm)] text-[color:var(--text)]"
                >
                  {providers.map((p) => (
                    <option key={p} value={p}>
                      {resolveProviderIdentity(p).label}
                    </option>
                  ))}
                </select>
                <Button
                  variant="ghost"
                  size="sm"
                  disabled={suggesting}
                  onClick={handleSuggest}
                  data-testid="suggest-competitors"
                >
                  {suggesting ? "Suggesting…" : "Suggest via AI"}
                </Button>
              </>
            ) : (
              <span className="text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
                Configure a provider key to enable AI suggestions.
              </span>
            )}
            {suggestError && (
              <span
                role="alert"
                data-testid="suggest-error"
                className="text-[length:var(--font-size-xs)] text-[color:var(--danger)]"
              >
                {suggestError}
              </span>
            )}
          </div>
        </div>
      </Card>

      <div className="flex items-center gap-[10px]">
        <Button
          variant="primary"
          size="sm"
          disabled={saving}
          onClick={handleSave}
          data-testid="brand-save"
        >
          {saving ? "Saving…" : "Save brand"}
        </Button>
        {saved && !restartRequired && (
          <span data-testid="brand-saved">
            <Pill mono tone="ok">
              Saved
            </Pill>
          </span>
        )}
        {restartRequired && (
          <span data-testid="brand-restart-required">
            <Pill mono tone="warn">
              Saved — restart API to apply new identity
            </Pill>
          </span>
        )}
        {error && (
          <span
            role="alert"
            data-testid="brand-error"
            className="text-[length:var(--font-size-xs)] text-[color:var(--danger)]"
          >
            {error}
          </span>
        )}
      </div>
    </div>
  );
}
