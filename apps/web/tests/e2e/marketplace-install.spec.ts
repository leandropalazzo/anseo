// Story 17.8 — marketplace install Sheet + unsigned Dialog + permissions gate.
//
// The install POST is mock-backed and the detail page fetches server-side, so
// the live walkthrough is marked test.fixme() per the red-phase ATDD
// convention. The Vitest suite (install-sheet) covers the gate + unsigned flow
// + signing-failure banner executing logic.
//
// Acceptance criteria traced:
//   AC-1 (UX-DR91): permissions render before [INSTALL →]; button gated
//   AC-2 (OQ-P3-26): single all-or-nothing acknowledgment
//   AC-3 (UX-DR101): unsigned-install ⚠ Dialog with confirmation
//   AC-4 (UX-DR92): signing-failure ErrorBanner
//   AC-5 (UX-DR95/127): success surfaces the Audit Event id
//   Axe: Sheet + Dialog have no a11y violations

import { test, expect } from "./fixtures";

test.describe("marketplace install", () => {
  test.fixme("permissions gate the install button", async ({ page }) => {
    await page.goto("/marketplace/community%2Fmarkdown-export");
    await page.getByTestId("open-install-sheet").click();
    await expect(page.getByTestId("install-permissions")).toBeVisible();
    await expect(page.getByTestId("install-confirm")).toBeDisabled();
    await page.getByTestId("install-acknowledge").check();
    await expect(page.getByTestId("install-confirm")).toBeEnabled();
  });

  test.fixme("unsigned install requires the ⚠ Dialog", async ({ page }) => {
    await page.goto("/marketplace/community%2Fmarkdown-export");
    await page.getByTestId("open-install-sheet").click();
    await page.getByTestId("install-acknowledge").check();
    await page.getByTestId("install-confirm").click();
    await expect(page.getByTestId("unsigned-dialog")).toBeVisible();
    await page.getByTestId("unsigned-confirm").click();
    await expect(page.getByTestId("install-audit-event")).toBeVisible();
  });

  test.fixme("install Sheet has no axe violations", async ({ page, axe }) => {
    await page.goto("/marketplace/community%2Fmarkdown-export");
    await page.getByTestId("open-install-sheet").click();
    await expect(page.getByTestId("install-sheet")).toBeVisible();
    await axe();
  });
});
