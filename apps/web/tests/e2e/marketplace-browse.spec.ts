// Story 17.7 — /marketplace browse + plugin detail.
//
// Both pages fetch the (mock-backed) plugin catalog server-side, so
// page.route() can't intercept; marked test.fixme() per the red-phase ATDD
// convention. The Vitest suite (plugin-format) provides executing coverage for
// emoji-strip, the capability block, and the trust badge.
//
// Acceptance criteria traced:
//   AC-1: /marketplace lists plugins; /marketplace/:slug shows detail
//   AC-2 (UX-DR90): verified vs unverified trust chips are distinct
//   AC-3 (UX-DR94): capability disclosure is always-visible (not collapsible)
//   AC-4 (UX-DR93): explicit pinned version (no implicit latest)
//   AC-5 (UX-DR97): emoji stripped to ▒
//   AC-6 (UX-DR98/99/100): zero-state, Installed-tab parity, update indicator
//   Axe: browse + detail have no a11y violations

import { test, expect } from "./fixtures";

test.describe("marketplace browse", () => {
  test.fixme("lists plugins with trust chips", async ({ page }) => {
    await page.goto("/marketplace");
    await expect(page.getByTestId("marketplace-list")).toBeVisible();
    await expect(page.getByTestId("plugin-trust").first()).toBeVisible();
  });

  test.fixme("Installed tab shows parity cards (UX-DR99)", async ({ page }) => {
    await page.goto("/marketplace");
    await page.getByTestId("marketplace-tab-installed").click();
    await expect(page.getByTestId("plugin-card").first()).toBeVisible();
  });

  test.fixme("detail shows pinned version + always-visible capabilities", async ({
    page,
  }) => {
    await page.goto("/marketplace/opengeo%2Fserp-enrichment");
    await expect(page.getByTestId("plugin-version")).toContainText(/^v\d/);
    await expect(page.getByTestId("plugin-capabilities")).toBeVisible();
  });

  test.fixme("emoji stripped to ▒ in the name (UX-DR97)", async ({ page }) => {
    await page.goto("/marketplace/opengeo%2Fserp-enrichment");
    await expect(page.getByTestId("plugin-name")).toContainText("▒");
  });

  test.fixme("browse has no axe violations", async ({ page, axe }) => {
    await page.goto("/marketplace");
    await expect(page.getByTestId("marketplace-page")).toBeVisible();
    await axe();
  });

  test.fixme("detail has no axe violations", async ({ page, axe }) => {
    await page.goto("/marketplace/opengeo%2Fserp-enrichment");
    await expect(page.getByTestId("plugin-detail-page")).toBeVisible();
    await axe();
  });
});
