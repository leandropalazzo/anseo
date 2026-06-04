// Story 19.9 — Overview "Top Recommendations" tile.
//
// The Overview page fetches GET /v1/recommendations server-side, so
// page.route() can't intercept it; marked test.fixme() per the red-phase ATDD
// convention. The cross-surface Vitest suite provides executing coverage of
// the shared-marker identity (UX-DR126).
//
// Acceptance criteria traced:
//   AC-1: Overview surfaces the top 3 active recs by priority
//   AC-2 (UX-DR126): markers match the /recommendations list rendering
//   Axe: tile introduces no a11y violations on the Overview

import { test, expect } from "./fixtures";

test.describe("overview top recommendations", () => {
  test.fixme("renders up to three top recs", async ({ page }) => {
    await page.goto("/");
    await expect(page.getByTestId("top-recs-list")).toBeVisible();
    const rows = page.getByTestId("top-rec-row");
    expect(await rows.count()).toBeLessThanOrEqual(3);
  });

  test.fixme("links through to the full recommendations surface", async ({
    page,
  }) => {
    await page.goto("/");
    await page.getByRole("link", { name: /All recommendations/i }).click();
    await expect(page).toHaveURL(/\/recommendations$/);
  });

  test.fixme("overview has no axe violations", async ({ page, axe }) => {
    await page.goto("/");
    await expect(page.getByTestId("overview-page")).toBeVisible();
    await axe();
  });
});
