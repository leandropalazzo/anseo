"use client";

import { Dialog } from "@base-ui/react/dialog";
import { useMemo, useState, useTransition } from "react";

import { ProviderDot } from "@/components/ui/provider-dot";
import {
  CONCRETE_PROVIDER_IDS,
  resolveProviderIdentity,
} from "@/lib/provider-colors";

const PROVIDER_OPTIONS = CONCRETE_PROVIDER_IDS;

/** Recurrence shapes the builder offers. The first three compile to the
 *  calendar grammar (`TZ=<tz> … at HH:MM`); `interval` keeps the legacy
 *  sub-daily frequency shorthands the backend already understood. */
type FreqMode = "daily" | "weekdays" | "everyN" | "interval";

/** Weekday tokens in calendar order; the value is what the backend grammar
 *  expects (`mon,wed,fri`). */
const WEEKDAYS: ReadonlyArray<{ value: string; label: string }> = [
  { value: "mon", label: "Mon" },
  { value: "tue", label: "Tue" },
  { value: "wed", label: "Wed" },
  { value: "thu", label: "Thu" },
  { value: "fri", label: "Fri" },
  { value: "sat", label: "Sat" },
  { value: "sun", label: "Sun" },
];

/** Legacy sub-daily / coarse cadences, for the "interval" mode. These fire on
 *  epoch-aligned UTC boundaries (no time-of-day) — kept for back-compat and
 *  high-frequency monitoring. */
const LEGACY_PRESETS: ReadonlyArray<{ label: string; value: string }> = [
  { label: "Hourly", value: "hourly" },
  { label: "Every 6 hours", value: "every 6 hours" },
  { label: "Every 30 minutes", value: "every 30 minutes" },
  { label: "Every 15 minutes", value: "every 15 minutes" },
  { label: "Weekly (UTC)", value: "weekly" },
];

type Submit = {
  name: string;
  cron: string;
  prompts: string[];
  providers: string[];
  debounce_minutes: number;
  allow_expensive: boolean;
};

async function createSchedule(payload: Submit) {
  // Same-origin proxy attaches the server-only X-OpenGEO-API-Key header; the
  // browser never sees the operator key.
  const r = await fetch(`/api/schedules`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  });
  if (!r.ok) {
    const body = await r.json().catch(() => null);
    const message =
      typeof body?.message === "string"
        ? body.message
        : `POST /api/schedules → ${r.status}`;
    throw new Error(message);
  }
  return r.json();
}

export function AddScheduleSheet({
  declaredPrompts,
  configuredProviders,
}: {
  declaredPrompts: ReadonlyArray<string>;
  /** Provider wire names that have a stored key; the create form defaults to
   *  fanning out across all of them. Falls back to `["openai"]` when none are
   *  configured yet so the form is never submittable with an empty set. */
  configuredProviders?: ReadonlyArray<string>;
}) {
  const defaultProviders =
    configuredProviders && configuredProviders.length > 0
      ? [...configuredProviders]
      : ["openai"];
  // Detected browser timezone, used as the default for calendar recurrences so
  // "9:00 AM" means the operator's local 9 AM, not UTC.
  const localTz = useMemo(() => {
    try {
      return Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC";
    } catch {
      return "UTC";
    }
  }, []);
  // The full IANA zone list when the runtime exposes it; otherwise just the
  // detected zone + UTC so the <select> is never empty.
  const timezones = useMemo(() => {
    const supported =
      typeof (Intl as { supportedValuesOf?: (k: string) => string[] })
        .supportedValuesOf === "function"
        ? (Intl as { supportedValuesOf: (k: string) => string[] }).supportedValuesOf(
            "timeZone",
          )
        : [];
    const set = new Set<string>(supported);
    set.add(localTz);
    set.add("UTC");
    return [...set].sort();
  }, [localTz]);

  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  const [freqMode, setFreqMode] = useState<FreqMode>("daily");
  const [timeOfDay, setTimeOfDay] = useState("09:00");
  const [timezone, setTimezone] = useState(localTz);
  const [weekdays, setWeekdays] = useState<string[]>(["mon", "tue", "wed", "thu", "fri"]);
  const [intervalDays, setIntervalDays] = useState("2");
  const [legacyPreset, setLegacyPreset] = useState("hourly");
  const [selectedPrompts, setSelectedPrompts] = useState<string[]>([]);
  const [selectedProviders, setSelectedProviders] = useState<string[]>(defaultProviders);
  const [debounce, setDebounce] = useState("5");

  const [allowExpensive, setAllowExpensive] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [isPending, startTransition] = useTransition();

  const toggleProvider = (p: string) => {
    setSelectedProviders((prev) =>
      prev.includes(p) ? prev.filter((x) => x !== p) : [...prev, p],
    );
  };
  const togglePrompt = (p: string) => {
    setSelectedPrompts((prev) =>
      prev.includes(p) ? prev.filter((x) => x !== p) : [...prev, p],
    );
  };

  const toggleWeekday = (d: string) => {
    setWeekdays((prev) =>
      prev.includes(d) ? prev.filter((x) => x !== d) : [...prev, d],
    );
  };

  // Compile the builder state into the backend's `cron` grammar. Calendar modes
  // emit `TZ=<tz> … at HH:MM`; interval mode passes the legacy shorthand through.
  const buildCron = (): string => {
    switch (freqMode) {
      case "daily":
        return `TZ=${timezone} daily at ${timeOfDay}`;
      case "weekdays": {
        const ordered = WEEKDAYS.filter((d) => weekdays.includes(d.value))
          .map((d) => d.value)
          .join(",");
        return `TZ=${timezone} weekly on ${ordered} at ${timeOfDay}`;
      }
      case "everyN":
        return `TZ=${timezone} every ${Number.parseInt(intervalDays, 10)} days at ${timeOfDay}`;
      case "interval":
        return legacyPreset;
    }
  };

  // Human-readable preview of the compiled recurrence, shown under the builder.
  const cronPreview = buildCron();

  const reset = () => {
    setName("");
    setFreqMode("daily");
    setTimeOfDay("09:00");
    setTimezone(localTz);
    setWeekdays(["mon", "tue", "wed", "thu", "fri"]);
    setIntervalDays("2");
    setLegacyPreset("hourly");
    setSelectedPrompts([]);
    setSelectedProviders(defaultProviders);
    setDebounce("5");
    setAllowExpensive(false);
    setError(null);
  };

  const submit = () => {
    setError(null);
    const debounceNum = Number.parseInt(debounce, 10);
    if (!name.trim()) return setError("`name` is required");
    if (selectedPrompts.length === 0) return setError("Pick at least one prompt");
    if (selectedProviders.length === 0) return setError("Pick at least one provider");
    if (freqMode === "weekdays" && weekdays.length === 0)
      return setError("Pick at least one weekday");
    if (freqMode === "everyN") {
      const n = Number.parseInt(intervalDays, 10);
      if (!Number.isFinite(n) || n < 1)
        return setError("Repeat interval must be a whole number of days ≥ 1");
    }
    if (!Number.isFinite(debounceNum) || debounceNum < 0)
      return setError("`debounce_minutes` must be a non-negative integer");
    startTransition(async () => {
      try {
        await createSchedule({
          name: name.trim(),
          cron: buildCron(),
          prompts: selectedPrompts,
          providers: selectedProviders,
          debounce_minutes: debounceNum,
          allow_expensive: allowExpensive,
        });
        setOpen(false);
        reset();
        // Force a server-component refresh so the new schedule appears
        // in the list without a full page reload.
        if (typeof window !== "undefined") window.location.reload();
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      }
    });
  };

  return (
    <Dialog.Root open={open} onOpenChange={setOpen}>
      <Dialog.Trigger
        data-testid="open-add-schedule"
        className="rounded border border-[color:var(--border)] bg-[color:var(--bg-elev)] px-3 py-1.5 text-sm font-medium text-[color:var(--text)] hover:bg-[color:var(--bg-elev-2)]"
      >
        + Add schedule
      </Dialog.Trigger>
      <Dialog.Portal>
        <Dialog.Backdrop className="fixed inset-0 bg-black/50 z-40" />
        <Dialog.Popup
          data-testid="add-schedule-sheet"
          className="fixed z-50 left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 w-[min(560px,90vw)] max-h-[90vh] overflow-auto rounded-md border border-[color:var(--border)] bg-[color:var(--bg-elev)] text-[color:var(--text)] p-5 shadow-lg"
        >
          <div className="flex items-start justify-between gap-3">
            <div>
              <Dialog.Title className="text-base font-semibold tracking-tight">
                New schedule
              </Dialog.Title>
              <Dialog.Description className="mt-1 text-sm text-[color:var(--text-muted)]">
                Declares a prompt × provider matrix on a recurrence. Density-cap
                and cost-cap checks fire server-side.
              </Dialog.Description>
            </div>
            <Dialog.Close className="text-[color:var(--text-muted)] hover:text-[color:var(--text)]">
              ×
            </Dialog.Close>
          </div>

          <form
            className="mt-4 space-y-4 text-sm"
            onSubmit={(e) => {
              e.preventDefault();
              submit();
            }}
          >
            <label className="block">
              <span className="block text-xs uppercase text-[color:var(--text-muted)] tracking-wide">
                Name
              </span>
              <input
                data-testid="field-name"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="weekday-mornings"
                pattern="[a-z0-9-]+"
                className="mt-1 w-full rounded border border-[color:var(--border)] bg-[color:var(--bg-sunken)] px-2 py-1 font-mono text-sm"
                autoFocus
                required
              />
              <span className="mt-1 block text-xs text-[color:var(--text-muted)]">
                Slug-safe (lowercase ASCII + digits + hyphens).
              </span>
            </label>

            <fieldset data-testid="recurrence-builder">
              <legend className="text-xs uppercase text-[color:var(--text-muted)] tracking-wide">
                Recurrence
              </legend>
              <div className="mt-1 inline-flex rounded border border-[color:var(--border)] overflow-hidden text-xs">
                {(
                  [
                    { value: "daily", label: "Daily" },
                    { value: "weekdays", label: "Specific days" },
                    { value: "everyN", label: "Every N days" },
                    { value: "interval", label: "Interval" },
                  ] as ReadonlyArray<{ value: FreqMode; label: string }>
                ).map((m) => (
                  <button
                    key={m.value}
                    type="button"
                    data-testid={`freq-${m.value}`}
                    aria-pressed={freqMode === m.value}
                    onClick={() => setFreqMode(m.value)}
                    className={`px-2.5 py-1 border-r border-[color:var(--border)] last:border-r-0 ${
                      freqMode === m.value
                        ? "bg-[color:var(--accent)] text-[color:var(--accent-contrast,#fff)]"
                        : "bg-[color:var(--bg-sunken)] text-[color:var(--text-muted)] hover:bg-[color:var(--bg-elev-2)]"
                    }`}
                  >
                    {m.label}
                  </button>
                ))}
              </div>

              {freqMode === "weekdays" ? (
                <div className="mt-2 flex flex-wrap gap-1">
                  {WEEKDAYS.map((d) => (
                    <button
                      key={d.value}
                      type="button"
                      data-testid={`weekday-${d.value}`}
                      aria-pressed={weekdays.includes(d.value)}
                      onClick={() => toggleWeekday(d.value)}
                      className={`rounded px-2 py-1 text-xs font-medium border ${
                        weekdays.includes(d.value)
                          ? "border-[color:var(--accent)] bg-[color:var(--accent)] text-[color:var(--accent-contrast,#fff)]"
                          : "border-[color:var(--border)] bg-[color:var(--bg-sunken)] text-[color:var(--text-muted)] hover:bg-[color:var(--bg-elev-2)]"
                      }`}
                    >
                      {d.label}
                    </button>
                  ))}
                </div>
              ) : null}

              {freqMode === "everyN" ? (
                <label className="mt-2 flex items-center gap-2 text-xs text-[color:var(--text-muted)]">
                  Repeat every
                  <input
                    data-testid="field-interval-days"
                    type="number"
                    min={1}
                    max={365}
                    value={intervalDays}
                    onChange={(e) => setIntervalDays(e.target.value)}
                    className="w-16 rounded border border-[color:var(--border)] bg-[color:var(--bg-sunken)] px-2 py-1 font-mono text-sm text-[color:var(--text)]"
                  />
                  days
                </label>
              ) : null}

              {freqMode === "interval" ? (
                <label className="mt-2 block">
                  <select
                    data-testid="field-legacy-preset"
                    value={legacyPreset}
                    onChange={(e) => setLegacyPreset(e.target.value)}
                    className="w-full rounded border border-[color:var(--border)] bg-[color:var(--bg-sunken)] px-2 py-1 text-[color:var(--text)]"
                  >
                    {LEGACY_PRESETS.map((p) => (
                      <option key={p.value} value={p.value}>
                        {p.label}
                      </option>
                    ))}
                  </select>
                  <span className="mt-1 block text-xs text-[color:var(--text-muted)]">
                    Fires on fixed UTC boundaries — no specific time of day.
                  </span>
                </label>
              ) : (
                <div className="mt-2 flex flex-wrap items-center gap-2">
                  <label className="flex items-center gap-1.5 text-xs text-[color:var(--text-muted)]">
                    at
                    <input
                      data-testid="field-time-of-day"
                      type="time"
                      value={timeOfDay}
                      onChange={(e) => setTimeOfDay(e.target.value)}
                      className="rounded border border-[color:var(--border)] bg-[color:var(--bg-sunken)] px-2 py-1 font-mono text-sm text-[color:var(--text)]"
                    />
                  </label>
                  <label className="flex items-center gap-1.5 text-xs text-[color:var(--text-muted)]">
                    in
                    <select
                      data-testid="field-timezone"
                      value={timezone}
                      onChange={(e) => setTimezone(e.target.value)}
                      className="max-w-[220px] rounded border border-[color:var(--border)] bg-[color:var(--bg-sunken)] px-2 py-1 text-sm text-[color:var(--text)]"
                    >
                      {timezones.map((tz) => (
                        <option key={tz} value={tz}>
                          {tz}
                        </option>
                      ))}
                    </select>
                  </label>
                </div>
              )}

              <p
                data-testid="recurrence-preview"
                className="mt-2 font-mono text-xs text-[color:var(--text-faint)]"
              >
                {cronPreview}
              </p>
            </fieldset>

            <fieldset>
              <legend className="text-xs uppercase text-[color:var(--text-muted)] tracking-wide">
                Prompts ({selectedPrompts.length})
              </legend>
              {declaredPrompts.length === 0 ? (
                <p className="mt-1 text-xs text-[color:var(--text-muted)]">
                  No prompts declared in this project. Add one with{" "}
                  <code className="font-mono">ogeo prompt add</code>.
                </p>
              ) : (
                <div className="mt-1 flex flex-wrap gap-1.5">
                  {declaredPrompts.map((p) => (
                    <label
                      key={p}
                      className="inline-flex items-center gap-1 rounded border border-[color:var(--border)] bg-[color:var(--bg-sunken)] px-2 py-0.5 font-mono text-xs"
                    >
                      <input
                        type="checkbox"
                        data-testid={`prompt-${p}`}
                        checked={selectedPrompts.includes(p)}
                        onChange={() => togglePrompt(p)}
                      />
                      {p}
                    </label>
                  ))}
                </div>
              )}
            </fieldset>

            <fieldset>
              <legend className="text-xs uppercase text-[color:var(--text-muted)] tracking-wide">
                Providers ({selectedProviders.length})
              </legend>
              <div className="mt-1 flex flex-wrap gap-1.5">
                {PROVIDER_OPTIONS.map((p) => (
                  <label
                    key={p}
                    className="inline-flex items-center gap-1.5 rounded border border-[color:var(--border)] bg-[color:var(--bg-sunken)] px-2 py-0.5 font-mono text-xs"
                  >
                    <input
                      type="checkbox"
                      data-testid={`provider-${p}`}
                      checked={selectedProviders.includes(p)}
                      onChange={() => toggleProvider(p)}
                    />
                    <ProviderDot provider={p} size={12} />
                    {resolveProviderIdentity(p).label}
                  </label>
                ))}
              </div>
            </fieldset>

            <label className="block">
              <span className="block text-xs uppercase text-[color:var(--text-muted)] tracking-wide">
                Debounce (minutes)
              </span>
              <input
                data-testid="field-debounce"
                type="number"
                min={0}
                max={120}
                value={debounce}
                onChange={(e) => setDebounce(e.target.value)}
                className="mt-1 w-24 rounded border border-[color:var(--border)] bg-[color:var(--bg-sunken)] px-2 py-1 font-mono text-sm"
              />
            </label>

            <label className="flex items-center gap-2 text-xs text-[color:var(--text-muted)]">
              <input
                data-testid="field-allow-expensive"
                type="checkbox"
                checked={allowExpensive}
                onChange={(e) => setAllowExpensive(e.target.checked)}
              />
              Ack projected monthly cost above the cap (cost-cap override).
            </label>

            {error ? (
              <p
                data-testid="add-schedule-error"
                className="rounded border border-[color:var(--danger)] bg-[color:var(--bg-sunken)] px-2 py-1 text-xs text-[color:var(--danger)]"
                role="alert"
              >
                {error}
              </p>
            ) : null}

            <div className="flex justify-end gap-2 pt-2 border-t border-[color:var(--border)]">
              <Dialog.Close className="rounded border border-[color:var(--border)] bg-[color:var(--bg-elev)] px-3 py-1.5 text-sm text-[color:var(--text-muted)] hover:bg-[color:var(--bg-elev-2)]">
                Cancel
              </Dialog.Close>
              <button
                type="submit"
                data-testid="submit-add-schedule"
                disabled={isPending}
                className="rounded bg-[color:var(--accent)] hover:opacity-90 disabled:opacity-60 px-3 py-1.5 text-sm font-medium text-[color:var(--accent-contrast,#fff)]"
              >
                {isPending ? "Creating…" : "Create"}
              </button>
            </div>
          </form>
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
