// Axe baseline (P0-023, UX-DR49): every Phase 1 dashboard surface passes
// WCAG 2.1 AA in its default render. Closes Epic 8 P0 (Accessibility).
//
// The pages render even when the API is unreachable — see
// `dashboard-shell.spec.ts` for the contract (api-error / empty-state banners
// are part of the AC). That means this suite also exercises the accessibility
// of empty + error states across the surfaces, per UX-DR49.
//
// trace: P0-023 (UX-DR49)
// trace: P2-021 (UX-DR52 color-contrast — axe enforces AA)

import { test } from "./fixtures";

const SURFACES: ReadonlyArray<{ name: string; path: string }> = [
  { name: "overview", path: "/" },
  { name: "runs list", path: "/runs" },
  { name: "visibility", path: "/visibility" },
  { name: "citations", path: "/citations" },
  { name: "settings", path: "/settings" },
  // Run-detail with a non-existent id exercises the error / 404 state, which
  // is still required to meet WCAG 2.1 AA per UX-DR49.
  { name: "run detail (not found)", path: "/runs/run-does-not-exist" },
];

for (const surface of SURFACES) {
  test(`@P0 @A11y ${surface.name} passes WCAG 2.1 AA`, async ({ page, axe }) => {
    await page.goto(surface.path);
    // Let the page settle — either real content, the api-error banner, an
    // empty-state banner, or the run-detail-error pane. Any of those is the
    // surface we want axe to evaluate.
    await page.waitForLoadState("networkidle");
    await axe();
  });
}
