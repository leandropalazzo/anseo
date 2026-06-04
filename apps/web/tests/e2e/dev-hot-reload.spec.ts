// Story 17.9 — /dev hot-reload flow.
//
// Marked test.fixme() per the red-phase ATDD convention (needs the dev-mode
// env + harness). Vitest (dev-surfaces) covers the atomic version flip +
// append-only logs executing logic.
//
// Acceptance criteria traced:
//   AC-3 (UX-DR121/125): hot-reload is atomic; in-flight count preserved
//   AC-4 (UX-DR122): logs append-only across a reload

import { test, expect } from "./fixtures";

test.describe("dev hot-reload", () => {
  test.fixme("flips the loaded version on reload", async ({ page }) => {
    await page.goto("/dev");
    const before = await page.getByTestId("dev-loaded-version").textContent();
    await page.getByTestId("dev-hot-reload").click();
    await expect(page.getByTestId("dev-loaded-version")).not.toHaveText(
      before ?? "",
    );
  });

  test.fixme("surfaces in-flight invocations mid-reload", async ({ page }) => {
    await page.goto("/dev");
    await page.getByTestId("dev-hot-reload").click();
    await expect(page.getByTestId("dev-in-flight")).toBeVisible();
  });
});
