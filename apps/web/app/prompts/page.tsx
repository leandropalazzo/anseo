"use client";

import { useEffect, useState, useTransition } from "react";
import { Plus, Trash2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { CodeBlock } from "@/components/ui/code-block";
import { Pill } from "@/components/ui/pill";
import { ICON_DEFAULTS } from "@/lib/icons";
import {
  listPrompts,
  createPrompt,
  updatePrompt,
  deletePrompt,
  suggestPrompts,
  type PromptView,
  type SuggestedPrompt,
  type SetupStatus,
} from "@/lib/api";
import {
  configuredCredentialProviderIds,
  resolveProviderIdentity,
} from "@/lib/provider-colors";

const inputClass =
  "w-full rounded-[6px] border border-[color:var(--hairline)] bg-[color:var(--surface)] px-[10px] py-[6px] text-[length:var(--font-size-sm)] text-[color:var(--text)]";

// A draft prompt has no id yet (never persisted); Save will create it.
const DRAFT_ID = "__draft__";

/** Parse a comma/newline-separated tag string into a deduped list. */
function tagsToList(s: string): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const raw of s.split(/[\n,]/)) {
    const t = raw.trim();
    if (!t || seen.has(t.toLowerCase())) continue;
    seen.add(t.toLowerCase());
    out.push(t);
  }
  return out;
}

export default function PromptsPage() {
  const [prompts, setPrompts] = useState<PromptView[]>([]);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [isDraft, setIsDraft] = useState(false);
  const [name, setName] = useState("");
  const [text, setText] = useState("");
  const [tags, setTags] = useState("");

  const [saving, startSaving] = useTransition();
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);

  // AI generator: pick a configured provider, get reviewable prompt drafts,
  // then add the ones the operator keeps. Nothing is persisted until "Add".
  const [providers, setProviders] = useState<string[]>([]);
  const [provider, setProvider] = useState("");
  const [suggesting, setSuggesting] = useState(false);
  const [suggestError, setSuggestError] = useState<string | null>(null);
  const [suggested, setSuggested] = useState<SuggestedPrompt[]>([]);
  const [adding, setAdding] = useState<string | null>(null);

  function selectPrompt(p: PromptView) {
    setIsDraft(false);
    setSelectedId(p.id);
    setName(p.name);
    setText(p.text);
    setTags(p.tags.join(", "));
    setError(null);
    setSaved(false);
  }

  async function reload(selectId?: string) {
    const list = await listPrompts();
    setPrompts(list);
    const target = selectId
      ? list.find((p) => p.id === selectId)
      : list.find((p) => p.id === selectedId) ?? list[0];
    if (target) selectPrompt(target);
    else {
      setSelectedId(null);
      setName("");
      setText("");
    }
  }

  useEffect(() => {
    let cancelled = false;
    listPrompts()
      .then((list) => {
        if (cancelled) return;
        setPrompts(list);
        if (list[0]) selectPrompt(list[0]);
        setLoading(false);
      })
      .catch((e) => {
        if (cancelled) return;
        setLoadError(String(e));
        setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // Only providers with a stored credential route can answer; list those for
  // the picker so OpenRouter is sent as `openrouter`, not as a concrete alias.
  useEffect(() => {
    let cancelled = false;
    fetch("/api/setup/status", { cache: "no-store" })
      .then((r) => (r.ok ? r.json() : Promise.reject(r.status)))
      .then((s: SetupStatus) => {
        if (cancelled) return;
        const configured = configuredCredentialProviderIds(s.api_keys);
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
    setSuggested([]);
    suggestPrompts(provider)
      .then((res) => {
        if (res.error) {
          setSuggestError(res.message ?? res.error);
          return;
        }
        // Drop drafts whose name collides with an existing prompt.
        const taken = new Set(prompts.map((p) => p.name.trim().toLowerCase()));
        setSuggested(
          res.prompts.filter((s) => !taken.has(s.name.trim().toLowerCase())),
        );
      })
      .catch((e) => setSuggestError(String(e)))
      .finally(() => setSuggesting(false));
  }

  function handleAddSuggested(s: SuggestedPrompt) {
    setSuggestError(null);
    setAdding(s.name);
    createPrompt(s.name, s.text, s.tags)
      .then(async (res) => {
        if (res.error) {
          setSuggestError(res.message ?? res.error);
          return;
        }
        setSuggested((prev) => prev.filter((x) => x !== s));
        await reload(res.id);
      })
      .catch((e) => setSuggestError(String(e)))
      .finally(() => setAdding(null));
  }

  const current = prompts.find((p) => p.id === selectedId) ?? null;
  const renamed = !isDraft && current !== null && name.trim() !== current.name;

  function handleNew() {
    setIsDraft(true);
    setSelectedId(DRAFT_ID);
    setName("");
    setText("");
    setTags("");
    setError(null);
    setSaved(false);
  }

  function handleSave() {
    const trimmedName = name.trim();
    if (!trimmedName) {
      setError("Prompt name must not be empty.");
      return;
    }
    if (!text.trim()) {
      setError("Prompt text must not be empty.");
      return;
    }
    setError(null);
    setSaved(false);
    const tagList = tagsToList(tags);
    startSaving(async () => {
      const res =
        isDraft || !current
          ? await createPrompt(trimmedName, text, tagList)
          : await updatePrompt(current.id, trimmedName, text, tagList);
      if (res.error) {
        setError(res.message ?? res.error);
        return;
      }
      setIsDraft(false);
      setSaved(true);
      await reload(res.id);
    });
  }

  function handleDelete() {
    if (isDraft || !current) {
      // Discard an unsaved draft.
      if (prompts[0]) selectPrompt(prompts[0]);
      else {
        setSelectedId(null);
        setIsDraft(false);
        setName("");
        setText("");
      }
      return;
    }
    setError(null);
    startSaving(async () => {
      const res = await deletePrompt(current.id);
      if (res.error) {
        setError(res.message ?? res.error);
        return;
      }
      await reload();
    });
  }

  return (
    <section
      data-testid="prompts-page"
      className="grid min-h-[600px] gap-[12px] [grid-template-columns:300px_1fr]"
    >
      {/* Left: prompt list */}
      <Card
        title="Prompts"
        eyebrow="tracked queries"
        padding={false}
        action={
          <Button
            size="sm"
            variant="ghost"
            onClick={handleNew}
            data-testid="prompt-new"
            leadingIcon={
              <Plus size={11} strokeWidth={ICON_DEFAULTS.strokeWidth} />
            }
          >
            New
          </Button>
        }
      >
        <div className="flex flex-col">
          {loading && (
            <p className="m-0 px-[14px] py-[10px] text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
              Loading…
            </p>
          )}
          {!loading && prompts.length === 0 && !isDraft && (
            <p className="m-0 px-[14px] py-[10px] text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
              No prompts yet. Click “New” to add one.
            </p>
          )}
          {isDraft && (
            <div
              className="flex flex-col gap-[4px] border-b border-[color:var(--hairline)] px-[14px] py-[10px]"
              style={{ borderLeft: "2px solid var(--accent)" }}
              data-testid="prompt-row-draft"
            >
              <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-sm)] text-[color:var(--text)]">
                {name.trim() || "new prompt"}
              </span>
              <Pill tone="info">unsaved draft</Pill>
            </div>
          )}
          {prompts.map((p) => {
            const active = !isDraft && selectedId === p.id;
            return (
              <button
                key={p.id}
                type="button"
                onClick={() => selectPrompt(p)}
                className={[
                  "cursor-pointer appearance-none border-0 border-b border-[color:var(--hairline)]",
                  "flex flex-col gap-[4px] px-[14px] py-[10px] text-left",
                  active
                    ? "bg-[color:var(--bg-elev-2)]"
                    : "bg-transparent hover:bg-[color:var(--bg-elev-2)]",
                ].join(" ")}
                style={{
                  borderLeft: active
                    ? "2px solid var(--accent)"
                    : "2px solid transparent",
                }}
                data-testid={`prompt-row-${p.name}`}
              >
                <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-sm)] text-[color:var(--text)]">
                  {p.name}
                </span>
                <div className="overflow-hidden text-ellipsis whitespace-nowrap text-[length:var(--font-size-xs)] text-[color:var(--text-muted)]">
                  {p.text}
                </div>
                {p.tags.length > 0 && (
                  <div className="flex flex-wrap gap-[4px]">
                    {p.tags.map((t) => (
                      <Pill key={t} mono tone={t === "AUTO" ? "info" : undefined}>
                        {t}
                      </Pill>
                    ))}
                  </div>
                )}
              </button>
            );
          })}
        </div>
      </Card>

      {/* Right: editor + AI generator */}
      <div className="flex flex-col gap-[12px]">
        <Card
          eyebrow="ai generator"
          title="Generate prompts"
          action={
            providers.length > 0 ? (
              <div className="flex items-center gap-[8px]">
                <select
                  data-testid="suggest-provider"
                  aria-label="Provider for AI prompt suggestions"
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
                  data-testid="suggest-prompts"
                >
                  {suggesting ? "Generating…" : "Suggest via AI"}
                </Button>
              </div>
            ) : undefined
          }
        >
          {providers.length === 0 ? (
            <p className="m-0 text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
              Configure a provider key in Settings to generate prompts grounded
              in your brand and competitors.
            </p>
          ) : (
            <div className="flex flex-col gap-[10px]">
              <p className="m-0 text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
                Ask a configured provider for tracking prompts. Review the
                drafts below and add the ones you want.
              </p>
              {suggestError && (
                <span
                  role="alert"
                  data-testid="suggest-error"
                  className="text-[length:var(--font-size-xs)] text-[color:var(--danger)]"
                >
                  {suggestError}
                </span>
              )}
              {suggested.length > 0 && (
                <div
                  className="flex flex-col gap-[8px]"
                  data-testid="suggested-prompts"
                >
                  {suggested.map((s) => (
                    <div
                      key={s.name}
                      className="flex items-start justify-between gap-[10px] border-b border-[color:var(--hairline)] pb-[8px]"
                      data-testid={`suggested-${s.name}`}
                    >
                      <div className="flex min-w-0 flex-col gap-[2px]">
                        <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-sm)] text-[color:var(--text)]">
                          {s.name}
                        </span>
                        <span className="text-[length:var(--font-size-xs)] text-[color:var(--text-muted)]">
                          {s.text}
                        </span>
                        {s.tags.length > 0 && (
                          <div className="flex flex-wrap gap-[4px]">
                            {s.tags.map((t) => (
                              <Pill
                                key={t}
                                mono
                                tone={t === "AUTO" ? "info" : undefined}
                              >
                                {t}
                              </Pill>
                            ))}
                          </div>
                        )}
                      </div>
                      <Button
                        variant="secondary"
                        size="sm"
                        disabled={adding === s.name}
                        onClick={() => handleAddSuggested(s)}
                        data-testid={`suggested-add-${s.name}`}
                        leadingIcon={
                          <Plus size={11} strokeWidth={ICON_DEFAULTS.strokeWidth} />
                        }
                      >
                        {adding === s.name ? "Adding…" : "Add"}
                      </Button>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}
        </Card>

        {selectedId === null && !loading ? (
          <Card eyebrow="prompt" title="No prompt selected">
            <p className="m-0 text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
              Select a prompt on the left, or create a new one.
            </p>
          </Card>
        ) : (
          <>
            <Card
              eyebrow={isDraft ? "new prompt" : "edit prompt"}
              title={name.trim() || (isDraft ? "new prompt" : "prompt")}
              action={
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={handleDelete}
                  disabled={saving}
                  data-testid="prompt-delete"
                  leadingIcon={
                    <Trash2 size={11} strokeWidth={ICON_DEFAULTS.strokeWidth} />
                  }
                >
                  {isDraft ? "Discard" : "Delete"}
                </Button>
              }
            >
              <div className="flex flex-col gap-[14px]">
                <label className="flex flex-col gap-[6px]">
                  <span className="text-[length:var(--font-size-sm)] text-[color:var(--text)]">
                    Name
                  </span>
                  <input
                    data-testid="prompt-name-input"
                    value={name}
                    onChange={(e) => setName(e.target.value)}
                    placeholder="brand-headline"
                    className={`${inputClass} font-[family-name:var(--font-mono)]`}
                    autoComplete="off"
                  />
                  {renamed && (
                    <span
                      role="status"
                      data-testid="prompt-rename-warning"
                      className="text-[length:var(--font-size-xs)] text-[color:var(--warn)]"
                    >
                      Renaming re-derives the prompt identity; allowed only
                      before its first run.
                    </span>
                  )}
                </label>

                <label className="flex flex-col gap-[6px]">
                  <span className="text-[length:var(--font-size-sm)] text-[color:var(--text)]">
                    Prompt text
                  </span>
                  <textarea
                    data-testid="prompt-text-input"
                    value={text}
                    onChange={(e) => setText(e.target.value)}
                    rows={6}
                    placeholder="Who makes the best widget?"
                    className={`${inputClass} resize-y`}
                  />
                </label>

                <label className="flex flex-col gap-[6px]">
                  <span className="text-[length:var(--font-size-sm)] text-[color:var(--text)]">
                    Tags
                  </span>
                  <input
                    data-testid="prompt-tags-input"
                    value={tags}
                    onChange={(e) => setTags(e.target.value)}
                    placeholder="comparison, alternatives (comma-separated)"
                    className={`${inputClass} font-[family-name:var(--font-mono)]`}
                    autoComplete="off"
                  />
                  {tagsToList(tags).length > 0 && (
                    <div className="flex flex-wrap gap-[6px]">
                      {tagsToList(tags).map((t) => (
                        <Pill key={t} mono tone={t === "AUTO" ? "info" : undefined}>
                          {t}
                        </Pill>
                      ))}
                    </div>
                  )}
                </label>

                <div className="flex items-center gap-[10px]">
                  <Button
                    variant="primary"
                    size="sm"
                    disabled={saving}
                    onClick={handleSave}
                    data-testid="prompt-save"
                  >
                    {saving ? "Saving…" : "Save"}
                  </Button>
                  {saved && (
                    <span data-testid="prompt-saved">
                      <Pill mono tone="ok">
                        Saved
                      </Pill>
                    </span>
                  )}
                  {error && (
                    <span
                      role="alert"
                      data-testid="prompt-error"
                      className="text-[length:var(--font-size-xs)] text-[color:var(--danger)]"
                    >
                      {error}
                    </span>
                  )}
                </div>
              </div>
            </Card>

            {!isDraft && text.trim() && (
              <Card eyebrow="run from CLI" title="Copy as command">
                <CodeBlock lang="bash" code={`ogeo run --prompt "${text}"`} />
              </Card>
            )}
          </>
        )}
      </div>

      {loadError && (
        <p
          role="alert"
          className="col-span-2 m-0 text-[length:var(--font-size-sm)] text-[color:var(--danger)]"
        >
          Failed to load prompts: {loadError}
        </p>
      )}
    </section>
  );
}
