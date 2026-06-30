"use client";

import { useEffect, useRef, useState, useTransition } from "react";
import { useRouter } from "next/navigation";

import { Icon, ICON_DEFAULTS } from "@/lib/icons";

interface ProjectView {
  project_id: string;
  name: string;
  created_at: string;
}

/**
 * Project switcher (Story 36.8) — header dropdown populated from
 * `GET /v1/projects` (via the same-origin `/api/projects` proxy). Selecting a
 * project POSTs to `/api/projects/select`, which sets the `anseo_project`
 * cookie; we then `router.refresh()` so every SSR fetch re-runs with the new
 * `X-Anseo-Project` header and the dashboard reflects the switch.
 *
 * The visual treatment matches the prototype's `ProjectSwitcher`: a bordered
 * trigger on `--bg-elev-2` with the deployment glyph + chevron.
 */
export function ProjectSwitcher({ deployment }: { deployment: "local" | "cloud" }) {
  const DeployIcon = deployment === "local" ? Icon.Server : Icon.Cloud;
  const router = useRouter();
  const [open, setOpen] = useState(false);
  const [projects, setProjects] = useState<ProjectView[]>([]);
  const [active, setActive] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [switching, startSwitch] = useTransition();
  const rootRef = useRef<HTMLDivElement>(null);

  function fetchProjects() {
    let cancelled = false;
    Promise.all([
      fetch("/api/projects", { cache: "no-store" })
        .then((r) => (r.ok ? r.json() : Promise.reject(r.status)))
        .then((j: { projects?: ProjectView[] }) => j.projects ?? []),
      fetch("/api/projects/select", { cache: "no-store" })
        .then((r) => (r.ok ? r.json() : Promise.reject(r.status)))
        .then((j: { name?: string | null }) => j.name ?? null),
    ])
      .then(([list, selected]) => {
        if (cancelled) return;
        setProjects(list);
        // Treat a persisted name as valid only if it appears in the current list.
        // A stale cookie (e.g. from a renamed/deleted project) would otherwise
        // send an unknown X-Anseo-Project and get a 404 from every API call.
        const validSelected =
          selected && list.some((p) => p.name === selected) ? selected : null;
        const effective = validSelected ?? list[0]?.name ?? null;
        setActive(effective);
        setLoaded(true);
        // Persist the effective project when the cookie was absent or stale.
        if (effective && effective !== selected) {
          fetch("/api/projects/select", {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ name: effective }),
          }).catch(() => {/* non-fatal */});
        }
      })
      .catch(() => {
        if (!cancelled) setLoaded(true);
      });
    return () => {
      cancelled = true;
    };
  }

  // Load the list + current selection once on mount.
  useEffect(fetchProjects, []);

  // Re-fetch whenever another part of the app mutates the project list.
  useEffect(() => {
    function onChanged() {
      fetchProjects();
    }
    window.addEventListener("anseo:projects-changed", onChanged);
    return () => window.removeEventListener("anseo:projects-changed", onChanged);
  }, []);

  // Close on outside click / Escape.
  useEffect(() => {
    if (!open) return;
    function onDown(e: MouseEvent) {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    }
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") setOpen(false);
    }
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  function select(name: string) {
    setOpen(false);
    if (name === active) return;
    setActive(name);
    startSwitch(async () => {
      await fetch("/api/projects/select", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ name }),
      });
      // Re-run all server components so data reflects the new project.
      router.refresh();
    });
  }

  const label = active ?? (loaded ? "No project" : "Loading…");
  const hasChoices = projects.length > 0;

  return (
    <div ref={rootRef} className="relative mt-[10px]">
      <button
        type="button"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-busy={switching}
        aria-label={switching ? `Switching to ${label}…` : `Active project: ${label}. Click to switch.`}
        disabled={!hasChoices}
        onClick={() => setOpen((v) => !v)}
        data-testid="project-switcher"
        className="flex w-full cursor-pointer items-center justify-between gap-[8px] border border-[color:var(--border)] bg-[color:var(--bg-elev-2)] px-[8px] py-[6px] text-left disabled:cursor-default"
      >
        <span className="flex min-w-0 items-center gap-[6px]">
          <DeployIcon
            size={12}
            strokeWidth={ICON_DEFAULTS.strokeWidth}
            className="text-[color:var(--text-muted)]"
          />
          <span
            data-testid="project-switcher-active"
            className="truncate font-[family-name:var(--font-body)] text-[length:var(--font-size-sm)] text-[color:var(--text)]"
          >
            {label}
          </span>
        </span>
        <Icon.ChevronDown
          size={12}
          strokeWidth={ICON_DEFAULTS.strokeWidth}
          className="text-[color:var(--text-faint)]"
        />
      </button>

      {open && hasChoices && (
        <ul
          role="listbox"
          data-testid="project-switcher-menu"
          aria-label="Select project"
          className="absolute left-0 right-0 top-[calc(100%+4px)] z-20 max-h-[240px] overflow-auto border border-[color:var(--border)] bg-[color:var(--bg-elev-2)] py-[2px] shadow-[0_8px_24px_rgba(0,0,0,0.32)]"
        >
          {projects.map((p) => {
            const isActive = p.name === active;
            return (
              <li key={p.project_id} role="none">
                <button
                  type="button"
                  role="option"
                  aria-selected={isActive}
                  onClick={() => select(p.name)}
                  data-testid={`project-option-${p.name}`}
                  className={[
                    "flex w-full cursor-pointer items-center justify-between gap-[8px] px-[8px] py-[6px] text-left font-[family-name:var(--font-body)] text-[length:var(--font-size-sm)]",
                    isActive
                      ? "bg-[color:var(--bg-sunken)] text-[color:var(--text)]"
                      : "bg-transparent text-[color:var(--text-muted)] hover:text-[color:var(--text)]",
                  ].join(" ")}
                >
                  <span className="truncate">{p.name}</span>
                  {isActive && (
                    <Icon.Check
                      size={12}
                      strokeWidth={ICON_DEFAULTS.strokeWidth}
                      className="text-[color:var(--accent)]"
                    />
                  )}
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
