// Story 19.8 — /recommendations list surface.
//
// The list page is a server component that fetches GET /v1/recommendations
// server-side, so page.route() cannot intercept it; these are marked
// test.fixme() per the repo red-phase ATDD convention until the e2e harness
// runs against a seeded API. The Vitest suites (recommendations-markers,
// lifecycle-actions) provide the executing coverage in the meantime.
//
// Acceptance criteria traced:
//   AC-1 (UX-DR107): active recs render with a mono priority + label
//   AC-3 (UX-DR106): ⚠ NDP marker on every non-deterministic rec
//   AC-6 (UX-DR111): bulk actions affordance present but disabled (CLI only)
//   Axe: list has no a11y violations

import { test, expect } from "./fixtures";

test.describe("recommendations list", () => {
  test.fixme("renders active recs with priority labels", async ({ page }) => {
    await page.goto("/recommendations");
    await expect(page.getByTestId("recommendations-list")).toBeVisible();
    const first = page.getByTestId("rec-priority").first();
    await expect(first).toHaveAttribute("data-severity", /high|medium|low|info/);
  });

  test.fixme("shows the NDP marker for non-deterministic recs", async ({
    page,
  }) => {
    await page.goto("/recommendations");
    await expect(page.getByTestId("rec-ndp-marker").first()).toContainText(
      "NDP",
    );
  });

  test.fixme("bulk actions are disabled (CLI only)", async ({ page }) => {
    await page.goto("/recommendations");
    await expect(page.getByTestId("rec-bulk-actions")).toBeDisabled();
  });

  test.fixme("list has no axe violations", async ({ page, axe }) => {
    await page.goto("/recommendations");
    await expect(page.getByTestId("recommendations-page")).toBeVisible();
    await axe();
  });
});
