// DB-authoritative prompt CRUD (Epic 35). Prompts are seeded from
// opengeo.yaml at boot, then edited through the dashboard. Identity
// (prompt_id) is a hash of the brand name folded with the prompt name, so a
// rename re-keys the row — allowed only before the first run (else 409).
//
// Called from client components, so these target the same-origin Next routes
// which attach the operator key server-side.

export interface PromptView {
  id: string;
  name: string;
  text: string;
  tags: string[];
  created_at: string;
}

export interface PromptMutationResult {
  id: string;
  name: string;
  text: string;
  tags: string[];
  /** True when the name changed and prompt_id was re-derived. */
  renamed: boolean;
  error?: string;
  message?: string;
}

export async function listPrompts(): Promise<PromptView[]> {
  const r = await fetch(`/api/prompts`, { method: "GET", cache: "no-store" });
  if (!r.ok) {
    throw new Error(`GET /api/prompts -> ${r.status} ${r.statusText}`);
  }
  // Tolerate a non-array body (e.g. an unseeded backend returning {}): the page
  // renders this directly, so a bad shape must degrade to empty, not crash.
  const data = await r.json();
  return Array.isArray(data) ? (data as PromptView[]) : [];
}

export async function createPrompt(
  name: string,
  text: string,
  tags: string[] = [],
): Promise<PromptMutationResult> {
  const r = await fetch(`/api/prompts`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ name, text, tags }),
    cache: "no-store",
  });
  const parsed = (await r.json()) as PromptMutationResult;
  if (!r.ok && !parsed.error) {
    parsed.error = `POST /api/prompts -> ${r.status} ${r.statusText}`;
  }
  return parsed;
}

export async function updatePrompt(
  id: string,
  name: string,
  text: string,
  tags: string[] = [],
): Promise<PromptMutationResult> {
  const r = await fetch(`/api/prompts/${encodeURIComponent(id)}`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ name, text, tags }),
    cache: "no-store",
  });
  const parsed = (await r.json()) as PromptMutationResult;
  if (!r.ok && !parsed.error) {
    parsed.error = `PUT /api/prompts/${id} -> ${r.status} ${r.statusText}`;
  }
  return parsed;
}

export interface SuggestedPrompt {
  name: string;
  text: string;
  tags: string[];
}

export interface SuggestPromptsResult {
  prompts: SuggestedPrompt[];
  provider: string;
  model: string;
  error?: string;
  message?: string;
}

/** Ask a configured provider for tracking prompts grounded in the brand. */
export async function suggestPrompts(
  provider: string,
): Promise<SuggestPromptsResult> {
  const r = await fetch(`/api/prompts/suggest`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ provider }),
    cache: "no-store",
  });
  const parsed = (await r.json()) as SuggestPromptsResult;
  if (!r.ok && !parsed.error) {
    parsed.error = `POST /api/prompts/suggest -> ${r.status} ${r.statusText}`;
  }
  return parsed;
}

export async function deletePrompt(
  id: string,
): Promise<{ error?: string; message?: string }> {
  const r = await fetch(`/api/prompts/${encodeURIComponent(id)}`, {
    method: "DELETE",
    cache: "no-store",
  });
  if (r.status === 204) return {};
  try {
    return (await r.json()) as { error?: string; message?: string };
  } catch {
    return { error: `DELETE /api/prompts/${id} -> ${r.status} ${r.statusText}` };
  }
}
