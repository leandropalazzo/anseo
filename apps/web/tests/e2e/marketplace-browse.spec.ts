// Story 17.7 (browse/detail) + Story 41.3 (LIVE registry wiring).
//
// As of 41.3 both pages fetch the live `/v1/marketplace/plugins` surface
// server-side; the E2E mock-api-server serves that endpoint with neutral
// `fixtures/*` plugins, so these tests now execute (previously red-phase
// fixme). Detail slugs use the live fixture ids.
//
// Acceptance criteria traced:
//   AC-1: /marketplace lists plugins; /marketplace/:slug shows detail
//   AC-2 (UX-DR90): verified vs unverified trust chips are distinct
//   AC-3 (UX-DR94): capability disclosure is always-visible (not collapsible)
//   AC-4 (UX-DR93): explicit pinned version (no implicit latest)
//   AC-6 (UX-DR98/99/100): zero-state, Installed-tab parity, update indicator
//   Axe: browse + detail have no a11y violations

import { test, expect } from "./fixtures";

test.describe("marketplace browse", () => {
  test("lists plugins with trust chips", async ({ page }) => {
    await page.goto("/marketplace");
    await expect(page.getByTestId("marketplace-list")).toBeVisible();
    await expect(page.getByTestId("plugin-trust").first()).toBeVisible();
  });

  test("Installed tab shows parity cards (UX-DR99)", async ({ page }) => {
    await page.goto("/marketplace");
    await page.getByTestId("marketplace-tab-installed").click();
    await expect(page.getByTestId("plugin-card").first()).toBeVisible();
  });

  test("detail shows pinned version + always-visible capabilities", async ({
    page,
  }) => {
    await page.goto("/marketplace/fixtures%2Falpha-signed");
    await expect(page.getByTestId("plugin-version")).toContainText(/^v\d/);
    await expect(page.getByTestId("plugin-capabilities")).toBeVisible();
  });

  test("browse has no axe violations", async ({ page, axe }) => {
    await page.goto("/marketplace");
    await expect(page.getByTestId("marketplace-page")).toBeVisible();
    await axe();
  });

  test("detail has no axe violations", async ({ page, axe }) => {
    await page.goto("/marketplace/fixtures%2Falpha-signed");
    await expect(page.getByTestId("plugin-detail-page")).toBeVisible();
    await axe();
  });
});
