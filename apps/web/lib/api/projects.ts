// Projects API client (Epic 36 — Stories 36.3 + 36.8).
//
// Wraps the operator-scoped `/v1/projects` endpoints: list active projects,
// create a project, and archive one. The wire shapes mirror
// `crates/wire-schema/openapi.json` (ProjectView / CreateProjectRequest /
// CreateProjectResponse).
//
// SSR/server callers hit the API directly via `getJson`; client components go
// through the same-origin `/api/projects` proxy (which attaches the server-only
// key + the selected-project header). The two paths share these types.

import { API_BASE_URL, getJson, setupHeaders } from "./_client";

/** A row from `GET /v1/projects` (`ProjectView`). */
export interface ProjectView {
  project_id: string;
  name: string;
  created_at: string;
}

/** `GET /v1/projects` envelope (`ProjectListResponse`). */
interface ProjectListResponse {
  projects: ProjectView[];
}

/** `POST /v1/projects` body (`CreateProjectRequest`). */
export interface CreateProjectInput {
  name: string;
  variants?: string[];
  site_url?: string | null;
}

/** `POST /v1/projects` response (`CreateProjectResponse`). */
export interface CreateProjectResult {
  project_id: string;
  name: string;
}

/** List active (non-archived) projects. Operator-scoped. */
export async function fetchProjects(): Promise<ProjectView[]> {
  const res = await getJson<ProjectListResponse>("/v1/projects");
  return res.projects;
}

/** Create a project. Server-only (carries the API key); used by the proxy. */
export async function createProject(
  input: CreateProjectInput,
): Promise<CreateProjectResult> {
  const r = await fetch(`${API_BASE_URL}/v1/projects`, {
    method: "POST",
    headers: await setupHeaders(true),
    body: JSON.stringify(input),
    cache: "no-store",
  });
  if (!r.ok) {
    throw new Error(`POST /v1/projects -> ${r.status} ${r.statusText}`);
  }
  return (await r.json()) as CreateProjectResult;
}

/** Archive a project by id. Server-only; used by the proxy. */
export async function archiveProject(id: string): Promise<void> {
  const r = await fetch(
    `${API_BASE_URL}/v1/projects/${encodeURIComponent(id)}/archive`,
    {
      method: "POST",
      headers: await setupHeaders(false),
      cache: "no-store",
    },
  );
  if (!r.ok) {
    throw new Error(
      `POST /v1/projects/${id}/archive -> ${r.status} ${r.statusText}`,
    );
  }
}
