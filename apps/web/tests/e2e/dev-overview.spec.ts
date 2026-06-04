// Story 17.9 — /dev plugin-author overview.
//
// /dev is gated by NEXT_PUBLIC_OGEO_DEV_MODE and renders server-side; marked
// test.fixme() per the red-phase ATDD convention (needs the dev-mode env +
// harness). Vitest (dev-surfaces) covers the banner, capability diff, and
// hot-reload logic.
//
// Acceptance criteria traced:
//   AC-1/2 (UX-DR119/120): dev banner present on /dev
//   AC-5 (UX-DR123): capability inspector visible
//   AC-6 (UX-DR124): Hosted Cloud renders the refusal, not the surface
//   Axe: overview has no a11y violations

import { test, expect } from "./fixtures";

test.describe("dev overview", () => {
  test.fixme("shows the dev-mode banner + capability inspector", async ({
    page,
  }) => {
    await page.goto("/dev");
    await expect(page.getByTestId("dev-mode-banner")).toBeVisible();
    await expect(page.getByTestId("capability-inspector")).toBeVisible();
  });

  test.fixme("refuses to render on Hosted Cloud (UX-DR124)", async ({
    page,
  }) => {
    // Harness sets NEXT_PUBLIC_OGEO_HOSTED_CLOUD=1 for this project.
    await page.goto("/dev");
    await expect(page.getByTestId("dev-hosted-refusal")).toBeVisible();
  });

  test.fixme("dev overview has no axe violations", async ({ page, axe }) => {
    await page.goto("/dev");
    await expect(page.getByTestId("dev-page")).toBeVisible();
    await axe();
  });
});
