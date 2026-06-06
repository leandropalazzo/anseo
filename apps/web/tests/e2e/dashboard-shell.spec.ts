// Smoke E2E for the Signal dashboard shell (UX-A..UX-F).
//
// Verifies that the Next.js app boots, renders every top-level page, and
// degrades gracefully when the API is unreachable. We don't depend on the
// API being up — the pages render with an `[data-testid="api-error"]` or
// `[data-testid="empty-state"]` banner in that case, which is itself part
// of the AC.

import { test, expect } from "./fixtures";

const PAGES: ReadonlyArray<{ path: string; testId: string }> = [
  { path: "/", testId: "overview-page" },
  { path: "/runs", testId: "runs-page" },
  { path: "/visibility", testId: "visibility-page" },
  { path: "/citations", testId: "citations-page" },
  { path: "/settings", testId: "settings-page" },
];

for (const p of PAGES) {
  test(`@P1 dashboard shell renders ${p.path}`, async ({ page }) => {
    await page.goto(p.path);
    await expect(
      page.getByTestId(p.testId),
      `page at ${p.path} should render with data-testid=${p.testId}`,
    ).toBeVisible();
    // Shell chrome should always be present (Signal: sidebar + topbar).
    await expect(page.getByTestId("app-sidebar")).toBeVisible();
    await expect(page.getByTestId("app-topbar")).toBeVisible();
    // Either real content or the api-error / empty-state banner — but never
    // a Next.js error overlay.
    await expect(page.locator("text=Application error")).toHaveCount(0);
  });
}

test("@P1 navigation works from overview to runs", async ({ page }) => {
  await page.goto("/");
  // Use the sidebar nav specifically — the Overview body also links to /runs
  // via an "All runs" affordance, which would otherwise be ambiguous.
  await page
    .getByTestId("app-sidebar")
    .getByRole("link", { name: "Runs" })
    .click();
  await expect(page).toHaveURL(/\/runs/);
  await expect(page.getByTestId("runs-page")).toBeVisible();
});

// 46.4: IA now has three groups — Monitor (core), Analyse (deep-dive), Operate (config).
test("@P1 sidebar groups monitor, analyse, and operate surfaces", async ({ page }) => {
  await page.goto("/");

  const monitor = page.getByTestId("nav-group-monitor");
  const analyse = page.getByTestId("nav-group-analyse");
  const operate = page.getByTestId("nav-group-operate");

  await expect(monitor).toContainText(
    "MonitorOverviewG ORunsG RVisibilityG VCitationsG CCompetitorsG KRecommendationsG DAlertsG A",
  );
  await expect(analyse).toContainText(
    "AnalyseSentimentG TAccuracyG YAuditG USite AnalyticsG ZCrawlersG W",
  );
  await expect(operate).toContainText(
    "OperatePromptsG PSchedulesG HMCPG MMarketplaceG BSettingsG S",
  );
  // Sentiment is a live surface (was a disabled "Soon" placeholder).
  await expect(page.getByTestId("nav-item-sentiment")).not.toHaveAttribute(
    "aria-disabled",
    "true",
  );
});
