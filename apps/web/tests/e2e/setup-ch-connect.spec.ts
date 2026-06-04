// Story 15.4 — ClickHouse remote-connect flow (Tinybird / Aiven /
// ClickHouse Cloud presets + Custom), mocked backend.
//
// The /setup/clickhouse/connect page has no server-side data fetch, so it
// renders without a backend; the connect POST is a client-side fetch and is
// mocked via page.route. Marked test.fixme() per the repo red-phase ATDD
// convention until the e2e harness runs in CI; the Vitest suite
// (connect-form.test.tsx) provides the executing coverage in the meantime.
//
// Acceptance criteria traced:
//   AC-1: preset radios + origin/username/password/database fields
//   AC-2: submit → POST /v1/setup/clickhouse/connect; success → /setup
//   AC-3: presets auto-fill canonical origin URLs (OQ-P3-23)
//   AC-4: invalid-creds / unreachable / schema-incompatible error banners

import { test, expect } from "./fixtures";
import type { ConnectResult } from "../../lib/api";

async function mockConnect(
  page: import("@playwright/test").Page,
  result: ConnectResult,
  status = 200,
): Promise<void> {
  await page.route("**/v1/setup/clickhouse/connect", (route) => {
    void route.fulfill({
      status,
      contentType: "application/json",
      body: JSON.stringify(result),
    });
  });
}

test.describe("setup ClickHouse remote-connect", () => {
  // AC-3 — Tinybird preset auto-fills the canonical origin
  test.fixme("Tinybird preset auto-fills origin URL", async ({ page }) => {
    await page.goto("/setup/clickhouse/connect");
    await page.getByTestId("ch-preset-tinybird").click();
    await expect(page.getByTestId("ch-endpoint-input")).toHaveValue(
      "https://api.tinybird.co",
    );
  });

  // AC-3 — Custom preset clears the origin for manual entry
  test.fixme("Custom preset clears origin URL", async ({ page }) => {
    await page.goto("/setup/clickhouse/connect");
    await page.getByTestId("ch-preset-custom").click();
    await expect(page.getByTestId("ch-endpoint-input")).toHaveValue("");
  });

  // AC-2 — successful connect redirects back to /setup
  test.fixme("successful connect redirects to /setup", async ({ page }) => {
    await mockConnect(page, {
      ok: true,
      state: "connected",
      message: "saved",
      endpoint: "https://abc.clickhouse.cloud:8443",
    });
    await page.goto("/setup/clickhouse/connect");
    await page.getByTestId("ch-username-input").fill("svc");
    await page.getByTestId("ch-password-input").fill("secret");
    await page.getByTestId("ch-connect-submit").click();
    await expect(page).toHaveURL(/\/setup$/);
  });

  // AC-4 — invalid credentials error banner
  test.fixme("invalid credentials shows error banner", async ({ page }) => {
    await mockConnect(page, {
      ok: false,
      state: "invalid_credentials",
      message: "rejected",
    });
    await page.goto("/setup/clickhouse/connect");
    await page.getByTestId("ch-connect-submit").click();
    const banner = page.getByTestId("ch-connect-error");
    await expect(banner).toBeVisible();
    await expect(banner).toHaveAttribute("data-state", "invalid_credentials");
  });

  // AC-4 — unreachable error banner
  test.fixme("unreachable shows error banner", async ({ page }) => {
    await mockConnect(page, {
      ok: false,
      state: "unreachable",
      message: "no route",
    });
    await page.goto("/setup/clickhouse/connect");
    await page.getByTestId("ch-connect-submit").click();
    await expect(page.getByTestId("ch-connect-error")).toHaveAttribute(
      "data-state",
      "unreachable",
    );
  });

  // AC-4 — schema-incompatible error banner
  test.fixme("schema-incompatible shows error banner", async ({ page }) => {
    await mockConnect(page, {
      ok: false,
      state: "schema_incompatible",
      message: "bad probe",
    });
    await page.goto("/setup/clickhouse/connect");
    await page.getByTestId("ch-connect-submit").click();
    await expect(page.getByTestId("ch-connect-error")).toHaveAttribute(
      "data-state",
      "schema_incompatible",
    );
  });

  // Axe — clean form has no a11y violations
  test.fixme("connect form has no axe violations", async ({ page, axe }) => {
    await page.goto("/setup/clickhouse/connect");
    await expect(page.getByTestId("ch-connect-form")).toBeVisible();
    await axe();
  });
});
