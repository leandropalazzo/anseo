// ATDD red-phase scaffold for Epic 1 / Story 1: Execute Prompt Run against a Provider.
//
// Mapped story: _bmad-output/planning-artifacts/stories/epic-1-story-1.md
// Mapped ACs: AC-6..AC-9 (partial-failure banner, role=status, error_kind subrow, axe pass)
// Mapped requirements: FR-2, UX-DR40, UX-DR22, UX-DR49
//
// Full scaffold + dev checklist: _bmad-output/test-artifacts/atdd-checklist-epic-1-story-1.md
//
// Expected state: RED. `/projects/demo/runs` does not exist yet; `POST /test/seed`
// does not exist yet. Dev brings both into existence per the checklist.
//
// Tests are marked `test.fixme` to keep CI green until the feature lands —
// mirrors the `#[ignore = "red-phase ATDD placeholder..."]` on the Rust
// analog at `apps/api/tests/prompt_run_smoke.rs`. When the prompt-run UI
// and the `POST /test/seed` route exist, remove the `.fixme` qualifier on
// each `test()` below.

import { test, expect } from "./fixtures";

const PROJECT_ID = "demo";

// Use existing webServer (`pnpm dev`) configured in playwright.config.ts.
// PLAYWRIGHT_SKIP_WEBSERVER=1 disables it for `playwright test --list` runs in CI.

test.beforeEach(async ({ request }) => {
  // POST /test/seed will live in apps/api behind ANSEO_TEST_MODE=1.
  // Seeds 2 OK runs (openai) + 2 failed runs (anthropic, provider_rate_limited).
  const response = await request.post(`/test/seed`, {
    data: {
      project_id: PROJECT_ID,
      runs: [
        { prompt: "p1", provider: "openai", status: "ok" },
        { prompt: "p2", provider: "openai", status: "ok" },
        {
          prompt: "p1",
          provider: "anthropic",
          status: "failed",
          error_kind: "provider_rate_limited",
        },
        {
          prompt: "p2",
          provider: "anthropic",
          status: "failed",
          error_kind: "provider_rate_limited",
        },
      ],
    },
  });
  expect(response.ok(), "AC-6 prereq: POST /test/seed must respond OK").toBeTruthy();
});

test.fixme("@P0 @Error AC-6 partial-failure banner appears above Runs table", async ({ page }) => {
  await page.goto(`/projects/${PROJECT_ID}/runs`);
  const banner = page
    .getByRole("status")
    .filter({ hasText: /partial.*failure|some.*runs.*failed/i });
  await expect(banner, "AC-6: PartialRunFailure banner visible").toBeVisible();

  const tableHandle = await page.getByRole("table").first().elementHandle();
  const bannerHandle = await banner.elementHandle();
  expect(tableHandle && bannerHandle, "AC-6: both banner + table must exist").toBeTruthy();

  const bannerPrecedesTable = await page.evaluate(
    ([b, t]) =>
      Boolean(
        b && t && (b as Element).compareDocumentPosition(t as Element) &
          Node.DOCUMENT_POSITION_FOLLOWING,
      ),
    [bannerHandle, tableHandle],
  );
  expect(bannerPrecedesTable, "AC-6: banner DOM-precedes table").toBeTruthy();
});

test.fixme("@P0 @Error AC-7 banner uses role='status' (not 'alert')", async ({ page }) => {
  await page.goto(`/projects/${PROJECT_ID}/runs`);
  await expect(
    page.locator('[role="status"]', { hasText: /partial.*failure|some.*runs.*failed/i }),
    "AC-7: role=status visible",
  ).toBeVisible();
  await expect(
    page.locator('[role="alert"]', { hasText: /partial.*failure/i }),
    "AC-7: must NOT use role=alert",
  ).toHaveCount(0);
});

test.fixme("@P0 @Error AC-8 failed rows show error_kind inline subrow", async ({ page }) => {
  await page.goto(`/projects/${PROJECT_ID}/runs`);
  await expect(
    page.getByText(/provider_rate_limited/i),
    "AC-8: error_kind text present for each failed row",
  ).toHaveCount(2);
});

test.fixme("@P0 @A11y AC-9 page passes WCAG 2.1 AA via axe", async ({ page, axe }) => {
  await page.goto(`/projects/${PROJECT_ID}/runs`);
  await page.getByRole("table").waitFor({ state: "visible" });
  await axe();
});
