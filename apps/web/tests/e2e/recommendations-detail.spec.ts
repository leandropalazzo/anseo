// Story 19.8 — /recommendations/:id detail surface.
//
// Server-component fetch (GET /v1/recommendations/:id) can't be intercepted by
// page.route(); marked test.fixme() per the red-phase ATDD convention. Vitest
// covers EvidenceChips + LifecycleActions executing logic.
//
// Acceptance criteria traced:
//   AC-2 (UX-DR105): traceability rendered as keyboard-reachable evidence links
//   AC-3 (UX-DR106/109): NDP marker + hard-outcome suppression disclaimer
//   AC-4 (UX-DR104): Snooze distinct from Dismiss
//   AC-5 (UX-DR108): acted-without-measurement loop flag
//   AC-7 (UX-DR103): empty traceability → render error
//   Axe: detail has no a11y violations

import { test, expect } from "./fixtures";

const REC_ID = "01JABCDEF0123456789ABCDEFG";

test.describe("recommendation detail", () => {
  test.fixme("evidence chips link to runs + citations", async ({ page }) => {
    await page.goto(`/recommendations/${REC_ID}`);
    await expect(page.getByTestId("rec-evidence")).toBeVisible();
    const run = page.getByTestId("rec-evidence-run").first();
    await expect(run).toHaveAttribute("href", /\/runs\//);
  });

  test.fixme("NDP recs show the suppression disclaimer", async ({ page }) => {
    await page.goto(`/recommendations/${REC_ID}`);
    await expect(page.getByTestId("rec-ndp-disclaimer")).toBeVisible();
  });

  test.fixme("snooze and dismiss are distinct actions", async ({ page }) => {
    await page.goto(`/recommendations/${REC_ID}`);
    await expect(page.getByTestId("rec-action-snooze")).toBeVisible();
    await expect(page.getByTestId("rec-action-dismiss")).toBeVisible();
  });

  test.fixme("empty traceability renders a render-error", async ({ page }) => {
    await page.goto(`/recommendations/${REC_ID}`);
    await expect(page.getByTestId("rec-evidence-error")).toBeVisible();
  });

  test.fixme("detail has no axe violations", async ({ page, axe }) => {
    await page.goto(`/recommendations/${REC_ID}`);
    await expect(page.getByTestId("rec-detail-page")).toBeVisible();
    await axe();
  });
});
