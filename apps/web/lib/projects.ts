// Selected-project plumbing (Story 36.8).
//
// The operator-selected project is persisted in an HTTP cookie so it survives
// reloads and is readable on the server during SSR. Per the Epic-36 contract
// (apps/api/src/extractors/project.rs) the `X-OpenGEO-Project` header is
// resolved *by brand name*, so the cookie stores the project NAME — the same
// value the switcher shows and the API resolves against the `projects` table.

/** Cookie carrying the selected project's brand name. */
export const PROJECT_COOKIE = "ogeo_project";

/** One year — the selection is sticky until the operator switches. */
export const PROJECT_COOKIE_MAX_AGE = 60 * 60 * 24 * 365;

/**
 * Read the selected project name from the request cookies during SSR / inside
 * a route handler. Returns `undefined` when nothing is selected yet, in which
 * case the API applies its own single-active-project fallback (ADR-004 tier 3).
 */
export async function getSelectedProject(): Promise<string | undefined> {
  const { cookies } = await import("next/headers");
  const store = await cookies();
  const value = store.get(PROJECT_COOKIE)?.value?.trim();
  return value ? value : undefined;
}
