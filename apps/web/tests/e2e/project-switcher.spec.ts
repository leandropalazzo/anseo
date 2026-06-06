// E2E for the Web Project Switcher (Story 36.8).
//
// Drives the real switcher against the canned mock API (tests/e2e/mock-api-server.mjs),
// which serves two seed projects (Acme, Globex) and project-scoped visibility
// data keyed by the X-Anseo-Project header. Switching projects must:
//   1. update the switcher's active label + the topbar breadcrumb, and
//   2. re-scope SSR data — the "Overall" visibility view's brand pill follows
//      the selection, proving the header threads through to the API.
//
// Settings CRUD (create + archive) is covered as a second test.

import { test, expect } from "./fixtures";
import type { Page } from "@playwright/test";

async function selectProject(page: Page, name: string) {
  await expect(page.getByTestId("project-switcher")).toBeEnabled();
  await page.getByTestId("project-switcher").click();
  const option = page.getByTestId(`project-option-${name}`);
  await expect(option).toBeVisible();
  const posted = page.waitForResponse(
    (r) =>
      r.url().includes("/api/projects/select") &&
      r.request().method() === "POST",
  );
  await option.click();
  await posted;
  await expect(page.getByTestId("project-switcher-active")).toHaveText(name);
}

async function brandOnOverall(page: Page) {
  await page.goto("/visibility");
  await expect(page.getByTestId("visibility-page")).toBeVisible();
  // The brand pill only renders on the "Overall" view.
  await page.getByRole("radio", { name: "Overall" }).click();
  return page.getByText(/^brand:\s/);
}

test("@P1 switching projects re-scopes dashboard data", async ({ page }) => {
  await page.goto("/");

  // Switch to Globex; the SSR-rendered Overall brand pill follows the header.
  await selectProject(page, "Globex");
  await expect(await brandOnOverall(page)).toHaveText("brand: Globex");

  // Switch to Acme and confirm both the label and the scoped data change.
  await page.goto("/");
  await selectProject(page, "Acme");
  await expect(await brandOnOverall(page)).toHaveText("brand: Acme");
});

test("@P1 settings can create and archive a project", async ({ page }) => {
  await page.goto("/settings");
  await page.getByTestId("settings-section-projects").click();
  await expect(page.getByTestId("settings-projects")).toBeVisible();

  // Seed projects are listed.
  await expect(page.getByTestId("project-row-Acme")).toBeVisible();

  // Create a new project.
  const unique = `Initech-${Date.now()}`;
  await page.getByTestId("project-name-input").fill(unique);
  await page.getByTestId("project-create").click();
  await expect(page.getByTestId(`project-row-${unique}`)).toBeVisible();

  // Archive it again — the row disappears from the active list.
  await page.getByTestId(`project-archive-${unique}`).click();
  await expect(page.getByTestId(`project-row-${unique}`)).toHaveCount(0);
});
