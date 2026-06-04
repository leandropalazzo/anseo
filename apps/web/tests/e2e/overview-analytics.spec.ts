import { test, expect } from "./fixtures";

test("@P1 overview renders live KPI, provider, and tag analytics", async ({
  page,
}) => {
  await page.goto("/");
  await expect(page.getByTestId("overview-page")).toBeVisible();

  await expect(page.getByText("Runs · last 7d", { exact: true })).toBeVisible();
  await expect(page.getByTestId("stat-tile-sparkline")).toHaveCount(4);
  await expect(page.getByText("Perplexity")).toBeVisible();
  await expect(page.getByTestId("tag-row-crm")).toContainText("86%");
});

test("@P1 citations page renders per-domain trend sparklines", async ({
  page,
}) => {
  await page.goto("/citations");
  await expect(page.getByTestId("citations-page")).toBeVisible();

  await expect(page.getByText("Footprint health")).toBeVisible();
  await expect(
    page.getByTestId("citation-trend-sparkline-example.com"),
  ).toBeVisible();
});
