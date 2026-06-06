// Story 17.8 (install Sheet) + Story 41.3 (LIVE install wiring).
//
// As of 41.3 the install POST goes through the same-origin `/api/plugins/install`
// proxy to `POST /v1/plugins/install`; the E2E mock-api-server serves that
// route (the unsigned `fixtures/beta-unsigned` plugin requires the ⚠ Dialog
// acknowledgment). These tests now execute (previously red-phase fixme).
//
// Acceptance criteria traced:
//   AC-1 (UX-DR91): permissions render before [INSTALL →]; button gated
//   AC-2 (OQ-P3-26): single all-or-nothing acknowledgment
//   AC-3 (UX-DR101): unsigned-install ⚠ Dialog with confirmation
//   AC-5 (UX-DR95/127): success surfaces the Audit Event id
//   Axe: Sheet + Dialog have no a11y violations

import { test, expect } from "./fixtures";

test.describe("marketplace install", () => {
  test("permissions gate the install button", async ({ page }) => {
    await page.goto("/marketplace/fixtures%2Fbeta-unsigned");
    await page.getByTestId("open-install-sheet").click();
    await expect(page.getByTestId("install-permissions")).toBeVisible();
    await expect(page.getByTestId("install-confirm")).toBeDisabled();
    await page.getByTestId("install-acknowledge").check();
    await expect(page.getByTestId("install-confirm")).toBeEnabled();
  });

  test("unsigned install requires the ⚠ Dialog", async ({ page }) => {
    await page.goto("/marketplace/fixtures%2Fbeta-unsigned");
    await page.getByTestId("open-install-sheet").click();
    await page.getByTestId("install-acknowledge").check();
    await page.getByTestId("install-confirm").click();
    await expect(page.getByTestId("unsigned-dialog")).toBeVisible();
    await page.getByTestId("unsigned-confirm").click();
    await expect(page.getByTestId("install-audit-event")).toBeVisible();
  });

  test("install Sheet has no axe violations", async ({ page, axe }) => {
    await page.goto("/marketplace/fixtures%2Fbeta-unsigned");
    await page.getByTestId("open-install-sheet").click();
    await expect(page.getByTestId("install-sheet")).toBeVisible();
    await axe();
  });
});
