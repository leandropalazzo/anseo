// Story 15.3 — ClickHouse local install flow (Docker detect → spawn →
// migrate → ETL → parity → green), mocked backend.
//
// Tests the ClickHouseCard install flow rendered on /setup. The initial
// /v1/setup/status probe is fetched server-side by the Next.js server
// component, which page.route cannot intercept; per the codebase red-phase
// ATDD convention these specs are marked test.fixme() until the harness
// gains a server-side fixture (or the real backend drives them in nightly CI).
//
// The install POST + SSE stream ARE client-side fetches and so are mockable.
//
// Acceptance criteria traced:
//   AC-1: ClickHouse section renders Docker detect result (present/absent/too-old)
//   AC-2: "Install locally" → POST /v1/setup/clickhouse/install, then SSE progress
//   AC-3: Progress states pulling → … → complete via progressbar + step pill
//   AC-4: Docker-absent branch routes to remote-connect with explainer copy

import { test, expect } from "./fixtures";
import type { SetupStatus, InstallStep } from "../../lib/api";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function baseStatus(overrides: Partial<SetupStatus> = {}): SetupStatus {
  return {
    postgres: {
      state: "healthy",
      schema_version: 1,
      row_count_estimate: 0,
      last_write_at: null,
    },
    clickhouse: {
      state: "not_configured",
      url: null,
      row_count: null,
      etl_lag_seconds: null,
    },
    worker: { state: "running", uptime_seconds: 1, queue_depth: 0 },
    webhook_target: {
      configured: false,
      last_delivery_at: null,
      last_status: null,
    },
    api_keys: [],
    docker: { present: true, version: "24.0.7" },
    ...overrides,
  };
}

async function mockSetupStatus(
  page: import("@playwright/test").Page,
  status: SetupStatus,
): Promise<void> {
  await page.route("**/v1/setup/status", (route) => {
    void route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify(status),
    });
  });
}

/** Build an SSE response body that walks the install steps. Each frame is
 *  `event: install\ndata: <json>\n\n`, matching apps/api setup.rs. */
function sseBody(steps: InstallStep[]): string {
  const total = steps.length;
  return (
    steps
      .map((step, i) => {
        const payload = JSON.stringify({
          step,
          progress: (i + 1) / total,
          log_line: `[mock] ${step}`,
          at: new Date().toISOString(),
        });
        return `event: install\ndata: ${payload}\n\n`;
      })
      .join("")
  );
}

async function mockInstall(
  page: import("@playwright/test").Page,
  steps: InstallStep[],
): Promise<{ postCount: () => number }> {
  let posts = 0;
  await page.route("**/v1/setup/clickhouse/install", (route) => {
    posts += 1;
    void route.fulfill({
      status: 202,
      contentType: "application/json",
      body: JSON.stringify({
        install_id: "01HZTEST",
        stream: "/v1/setup/clickhouse/install-stream?id=01HZTEST",
      }),
    });
  });
  await page.route("**/v1/setup/clickhouse/install-stream**", (route) => {
    void route.fulfill({
      status: 200,
      contentType: "text/event-stream",
      body: sseBody(steps),
    });
  });
  return { postCount: () => posts };
}

const ALL_STEPS: InstallStep[] = [
  "docker_detected",
  "image_pulling",
  "container_starting",
  "provisioning_user",
  "applying_migrations",
  "running_parity_test",
  "complete",
];

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

test.describe("setup ClickHouse local install", () => {
  // AC-1 — Docker present detect result
  test.fixme("renders Docker present verdict", async ({ page }) => {
    await mockSetupStatus(page, baseStatus());
    await page.goto("/setup");

    const verdict = page.getByTestId("ch-docker-verdict");
    await expect(verdict).toBeVisible();
    await expect(verdict).toHaveAttribute("data-verdict", "present");
    await expect(page.getByTestId("ch-install-button")).toBeVisible();
  });

  // AC-1 — Docker too old
  test.fixme("renders Docker too-old verdict", async ({ page }) => {
    await mockSetupStatus(
      page,
      baseStatus({ docker: { present: true, version: "18.09.1" } }),
    );
    await page.goto("/setup");

    const verdict = page.getByTestId("ch-docker-verdict");
    await expect(verdict).toHaveAttribute("data-verdict", "too_old");
    // No install button; remote-connect CTA shown instead.
    await expect(page.getByTestId("ch-install-button")).toHaveCount(0);
    await expect(page.getByTestId("ch-remote-connect-button")).toBeVisible();
  });

  // AC-2 / AC-3 — install in-progress → complete via SSE
  test.fixme(
    "install walks progress to complete",
    async ({ page }) => {
      await mockSetupStatus(page, baseStatus());
      const install = await mockInstall(page, ALL_STEPS);
      await page.goto("/setup");

      await page.getByTestId("ch-install-button").click();
      expect(install.postCount()).toBe(1);

      const progress = page.getByTestId("ch-install-progress");
      await expect(progress).toBeVisible();

      // Terminal step renders complete state + 100%.
      await expect(page.getByTestId("ch-install-complete")).toBeVisible();
      await expect(page.getByTestId("ch-install-step")).toContainText(
        "Complete",
      );
      const bar = progress.getByRole("progressbar");
      await expect(bar).toHaveAttribute("aria-valuenow", "100");
    },
  );

  // AC-4 — Docker absent routes to remote-connect with explainer copy
  test.fixme("Docker absent routes to remote-connect", async ({ page }) => {
    await mockSetupStatus(
      page,
      baseStatus({ docker: { present: false, version: null } }),
    );
    await page.goto("/setup");

    const cta = page.getByTestId("ch-remote-connect-cta");
    await expect(cta).toBeVisible();
    await expect(cta).toContainText("Docker isn't available");

    await page.getByTestId("ch-remote-connect-button").click();
    await expect(page).toHaveURL(/\/setup\/clickhouse\/connect$/);
  });

  // Axe — no a11y violations in the install-present default state
  test.fixme("install state has no axe violations", async ({ page, axe }) => {
    await mockSetupStatus(page, baseStatus());
    await mockInstall(page, ALL_STEPS);
    await page.goto("/setup");
    await expect(page.getByTestId("ch-install-button")).toBeVisible();
    await axe();
  });
});
