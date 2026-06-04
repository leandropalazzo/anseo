// Story 10.4 — AddScheduleSheet sanity coverage.
//
// Schedules now live in their own Operate section at /schedules (was
// previously the third tab on /alerts). This spec navigates there, opens
// the AddScheduleSheet and exercises the same validation paths as before
// (no prompts → error, Esc closes).

import { test, expect } from "./fixtures";

test("@P1 schedule sheet opens, validates, and closes", async ({ page }) => {
  await page.goto("/schedules");
  await expect(page.getByTestId("schedules-page")).toBeVisible();

  const trigger = page.getByTestId("open-add-schedule");
  await expect(trigger).toBeVisible();
  await trigger.click();

  const sheet = page.getByTestId("add-schedule-sheet");
  await expect(sheet).toBeVisible();

  // Filling a name but no prompts should surface the client-side validation
  // error before any fetch is attempted.
  await page.getByTestId("field-name").fill("daily-fixture");
  await page.getByTestId("provider-openai").check();
  await page.getByTestId("submit-add-schedule").click();
  await expect(page.getByTestId("add-schedule-error")).toContainText("prompt");

  // Esc closes the dialog.
  await page.keyboard.press("Escape");
  await expect(sheet).toBeHidden();
});
