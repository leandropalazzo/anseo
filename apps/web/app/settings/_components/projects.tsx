"use client";

import { useEffect, useState, useTransition } from "react";

import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Pill } from "@/components/ui/pill";

interface ProjectView {
  project_id: string;
  name: string;
  created_at: string;
}

const inputClass =
  "flex-1 rounded-[6px] border border-[color:var(--hairline)] bg-[color:var(--surface)] px-[10px] py-[6px] text-[length:var(--font-size-sm)] text-[color:var(--text)]";

/**
 * Projects settings (Story 36.8) — create + archive projects via the 36.3
 * endpoints, proxied through `/api/projects`. The header switcher reads the
 * same list, so creating/archiving here reflows the dropdown on next load.
 */
export function ProjectsSection() {
  const [projects, setProjects] = useState<ProjectView[]>([]);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);

  const [name, setName] = useState("");
  const [siteUrl, setSiteUrl] = useState("");
  const [creating, startCreating] = useTransition();
  const [createError, setCreateError] = useState<string | null>(null);

  const [busyId, setBusyId] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  async function load() {
    try {
      const r = await fetch("/api/projects", { cache: "no-store" });
      if (!r.ok) throw new Error(`HTTP ${r.status}`);
      const j = (await r.json()) as { projects?: ProjectView[] };
      setProjects(j.projects ?? []);
      setLoadError(null);
    } catch (e) {
      setLoadError(String(e));
    } finally {
      setLoaded(true);
    }
  }

  useEffect(() => {
    let cancelled = false;
    fetch("/api/projects", { cache: "no-store" })
      .then((r) => (r.ok ? r.json() : Promise.reject(r.status)))
      .then((j: { projects?: ProjectView[] }) => {
        if (cancelled) return;
        setProjects(j.projects ?? []);
        setLoadError(null);
      })
      .catch((e) => {
        if (!cancelled) setLoadError(String(e));
      })
      .finally(() => {
        if (!cancelled) setLoaded(true);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  function handleCreate() {
    const trimmed = name.trim();
    if (!trimmed) {
      setCreateError("Project name must not be empty.");
      return;
    }
    setCreateError(null);
    startCreating(async () => {
      const r = await fetch("/api/projects", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          name: trimmed,
          site_url: siteUrl.trim() || undefined,
        }),
      });
      if (!r.ok) {
        const body = await r.text().catch(() => "");
        setCreateError(`Create failed (HTTP ${r.status}). ${body}`.trim());
        return;
      }
      setName("");
      setSiteUrl("");
      await load();
      window.dispatchEvent(new CustomEvent("anseo:projects-changed"));
    });
  }

  async function handleArchive(p: ProjectView) {
    setActionError(null);
    setBusyId(p.project_id);
    try {
      const r = await fetch(
        `/api/projects/${encodeURIComponent(p.project_id)}/archive`,
        { method: "POST" },
      );
      if (!r.ok) throw new Error(`HTTP ${r.status}`);
      await load();
      window.dispatchEvent(new CustomEvent("anseo:projects-changed"));
    } catch (e) {
      setActionError(`Archive failed: ${String(e)}`);
    } finally {
      setBusyId(null);
    }
  }

  return (
    <div className="flex flex-col gap-[16px]" data-testid="settings-projects">
      <Card eyebrow="workspace" title="Projects">
        <p className="m-0 mb-[12px] text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
          Each project is an isolated brand workspace. The header switcher scopes
          every dashboard read to the selected project.
        </p>
        <div className="flex flex-col gap-[10px]" data-testid="projects-list">
          {!loaded && (
            <p className="m-0 text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
              Loading…
            </p>
          )}
          {loaded && loadError && (
            <p
              role="alert"
              data-testid="projects-load-error"
              className="m-0 text-[length:var(--font-size-sm)] text-[color:var(--danger)]"
            >
              Failed to load projects: {loadError}
            </p>
          )}
          {loaded && !loadError && projects.length === 0 && (
            <p className="m-0 text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
              No active projects yet.
            </p>
          )}
          {projects.map((p) => (
            <div
              key={p.project_id}
              data-testid={`project-row-${p.name}`}
              className="flex items-center justify-between gap-[8px] border-b border-[color:var(--hairline)] pb-[8px]"
            >
              <span className="flex min-w-0 flex-col">
                <span className="truncate text-[length:var(--font-size-sm)] text-[color:var(--text)]">
                  {p.name}
                </span>
                <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
                  {p.project_id}
                </span>
              </span>
              <Button
                variant="ghost"
                size="sm"
                disabled={busyId === p.project_id}
                onClick={() => handleArchive(p)}
                data-testid={`project-archive-${p.name}`}
              >
                {busyId === p.project_id ? "Archiving…" : "Archive"}
              </Button>
            </div>
          ))}
          {actionError && (
            <span
              role="alert"
              data-testid="project-action-error"
              className="text-[length:var(--font-size-xs)] text-[color:var(--danger)]"
            >
              {actionError}
            </span>
          )}
        </div>
      </Card>

      <Card eyebrow="new workspace" title="Create project">
        <div className="flex flex-col gap-[14px]">
          <label className="flex flex-col gap-[6px]">
            <span className="text-[length:var(--font-size-sm)] text-[color:var(--text)]">
              Project name
            </span>
            <input
              data-testid="project-name-input"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="Acme"
              autoComplete="off"
              className={inputClass}
            />
          </label>
          <label className="flex flex-col gap-[6px]">
            <span className="text-[length:var(--font-size-sm)] text-[color:var(--text)]">
              Website URL
            </span>
            <input
              data-testid="project-site-url-input"
              value={siteUrl}
              onChange={(e) => setSiteUrl(e.target.value)}
              placeholder="https://example.com"
              autoComplete="off"
              className={inputClass}
            />
          </label>
          <div className="flex items-center gap-[10px]">
            <Button
              variant="primary"
              size="sm"
              disabled={creating}
              onClick={handleCreate}
              data-testid="project-create"
            >
              {creating ? "Creating…" : "Create project"}
            </Button>
            {createError && (
              <span
                role="alert"
                data-testid="project-create-error"
                className="text-[length:var(--font-size-xs)] text-[color:var(--danger)]"
              >
                {createError}
              </span>
            )}
            {!createError && loaded && projects.length > 0 && (
              <Pill mono tone="ok">
                {projects.length} active
              </Pill>
            )}
          </div>
        </div>
      </Card>
    </div>
  );
}
