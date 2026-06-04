// Story 14.6 — accessibility verification in BOTH themes.
//
// Walks every Phase 1 dashboard route in light AND dark theme, runs
// @axe-core/playwright (WCAG2A/AA + WCAG21A/AA per fixtures.ts), and
// asserts zero violations on either render.
//
// Pages render an `[data-testid="api-error"]` or `[data-testid="empty-state"]`
// banner when the API isn't up (per Phase 1 dashboard-shell AC), so this
// spec runs without a live backend — axe verifies the rendered DOM, not
// the data.
//
// The dashboard exposes a `[data-testid="theme-toggle"]` button that
// flips `<html data-theme>` between "dark" and "light"; the spec uses
// direct attribute set rather than the button click so each theme is
// asserted from a known-good baseline regardless of the toggle's prior
// state.

import { test, expect } from "./fixtures";

const ROUTES: ReadonlyArray<{ path: string; testId: string }> = [
  { path: "/", testId: "overview-page" },
  { path: "/runs", testId: "runs-page" },
  { path: "/visibility", testId: "visibility-page" },
  { path: "/citations", testId: "citations-page" },
  { path: "/alerts", testId: "alerts-page" },
  { path: "/prompts", testId: "prompts-page" },
  { path: "/competitors", testId: "competitors-page" },
  { path: "/mcp", testId: "mcp-page" },
  { path: "/settings", testId: "settings-page" },
  { path: "/setup", testId: "setup-page" },
];

const THEMES = ["light", "dark"] as const;

for (const route of ROUTES) {
  for (const theme of THEMES) {
    test(`@axe-both-themes ${route.path} renders WCAG-clean in ${theme}`, async ({
      page,
      axe,
    }) => {
      await page.addInitScript((t) => {
        try {
          window.localStorage.setItem("ogeo-theme", t);
        } catch {
          /* sandboxed contexts may forbid localStorage; init script also sets attr */
        }
      }, theme);

      await page.goto(route.path);

      await page.evaluate((t) => {
        document.documentElement.setAttribute("data-theme", t);
      }, theme);

      await expect(page.getByTestId(route.testId)).toBeVisible();
      await expect(
        page.locator("html"),
        `data-theme should be ${theme} before axe runs`,
      ).toHaveAttribute("data-theme", theme);

      await axe();
    });
  }
}

test("@axe-both-themes theme toggle round-trips and persists", async ({
  page,
}) => {
  await page.goto("/");
  await expect(page.getByTestId("overview-page")).toBeVisible();

  const html = page.locator("html");
  const toggle = page.getByTestId("theme-toggle");

  const initial = await html.getAttribute("data-theme");
  expect(initial === "light" || initial === "dark").toBe(true);

  await toggle.click();
  const flipped = initial === "dark" ? "light" : "dark";
  await expect(html).toHaveAttribute("data-theme", flipped);

  const persisted = await page.evaluate(() =>
    window.localStorage.getItem("ogeo-theme"),
  );
  expect(persisted).toBe(flipped);

  await page.reload();
  await expect(html).toHaveAttribute("data-theme", flipped);
});
