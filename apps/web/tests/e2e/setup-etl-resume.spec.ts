// Story 15.5 — ClickHouse ETL progress surface (mocked, Story 0.1 deferred).
//
// Tests the EtlProgressCard component rendered on /setup.
// The component is integrated into /setup in a follow-up task; until then
// tests are marked fixme (red-phase ATDD pattern per the codebase convention).
//
// When Story 0.1 ships and /v1/setup/clickhouse/status is live, swap the
// page.route() mock for the real endpoint and remove the fixme qualifiers.
//
// Acceptance criteria traced:
//   AC-1: ETL progress reads from GET /v1/setup/clickhouse/status
//         and renders <batches_done> / <batches_total> with a ProgressBar
//   AC-2: On "interrupted" state, [Resume] button is visible
//   AC-3: Resume triggers POST /v1/setup/clickhouse/resume and UI updates

import { test, expect } from "./fixtures";
import type { ClickHouseEtlStatus } from "../../lib/api";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Inject an ETL status response via page.route, intercepting BOTH the
 *  Next.js /v1 proxy and the direct API call so the server-component data
 *  reaches the client regardless of how the page fetches it. */
async function mockEtlStatus(
  page: import("@playwright/test").Page,
  status: ClickHouseEtlStatus,
): Promise<void> {
  // Intercept the backend API endpoint (called by the Next.js server component)
  await page.route("**/v1/setup/clickhouse/status", (route) => {
    void route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify(status),
    });
  });
  // Intercept the resume endpoint to prevent accidental real calls
  await page.route("**/v1/setup/clickhouse/resume", (route) => {
    void route.fulfill({
      status: 202,
      contentType: "application/json",
      body: JSON.stringify({ ok: true }),
    });
  });
}

// Timestamp far enough in the past to be considered "interrupted" (>90 s ago)
function oldHeartbeat(): string {
  return new Date(Date.now() - 120_000).toISOString();
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

test.describe("setup ETL progress", () => {
  // AC-1 / fresh-start — state "idle", no progress fraction shown
  test.fixme(
    "fresh-start shows idle state",
    async ({ page }) => {
      const status: ClickHouseEtlStatus = {
        state: "idle",
        batches_done: null,
        batches_total: null,
        last_heartbeat_at: null,
        finished_at: null,
        error: null,
      };
      await mockEtlStatus(page, status);
      await page.goto("/setup");

      const card = page.getByTestId("etl-progress-card");
      await expect(card).toBeVisible();

      const badge = page.getByTestId("etl-state-badge");
      await expect(badge).toContainText("idle");

      // No progress fraction displayed in idle state
      await expect(page.getByTestId("etl-progress")).toHaveCount(0);

      // Resume button must NOT appear in idle state
      await expect(page.getByTestId("etl-resume-button")).toHaveCount(0);
    },
  );

  // AC-1 / in-progress — state "running", shows batches fraction
  test.fixme(
    "in-progress shows batches fraction",
    async ({ page }) => {
      const status: ClickHouseEtlStatus = {
        state: "running",
        batches_done: 150,
        batches_total: 400,
        last_heartbeat_at: new Date().toISOString(),
        finished_at: null,
        error: null,
      };
      await mockEtlStatus(page, status);
      await page.goto("/setup");

      const card = page.getByTestId("etl-progress-card");
      await expect(card).toBeVisible();

      const badge = page.getByTestId("etl-state-badge");
      await expect(badge).toContainText("running");

      const progress = page.getByTestId("etl-progress");
      await expect(progress).toBeVisible();
      await expect(progress).toContainText("150");
      await expect(progress).toContainText("400");

      // Resume button must NOT appear while running
      await expect(page.getByTestId("etl-resume-button")).toHaveCount(0);
    },
  );

  // AC-2 — "interrupted" state: last_heartbeat_at > 90 s ago, finished_at null
  //         Resume button must be visible
  test.fixme(
    "interrupted shows Resume button",
    async ({ page }) => {
      const status: ClickHouseEtlStatus = {
        state: "interrupted",
        batches_done: 200,
        batches_total: 400,
        last_heartbeat_at: oldHeartbeat(),
        finished_at: null,
        error: null,
      };
      await mockEtlStatus(page, status);
      await page.goto("/setup");

      const card = page.getByTestId("etl-progress-card");
      await expect(card).toBeVisible();

      const badge = page.getByTestId("etl-state-badge");
      await expect(badge).toContainText("interrupted");

      const progress = page.getByTestId("etl-progress");
      await expect(progress).toBeVisible();
      await expect(progress).toContainText("200");
      await expect(progress).toContainText("400");

      const resumeBtn = page.getByTestId("etl-resume-button");
      await expect(resumeBtn).toBeVisible();
      await expect(resumeBtn).toBeEnabled();
    },
  );

  // AC-3 — clicking Resume triggers POST /v1/setup/clickhouse/resume
  //         and the UI transitions to "running"
  test.fixme(
    "resume triggers running state",
    async ({ page }) => {
      // First render: interrupted
      const interrupted: ClickHouseEtlStatus = {
        state: "interrupted",
        batches_done: 200,
        batches_total: 400,
        last_heartbeat_at: oldHeartbeat(),
        finished_at: null,
        error: null,
      };

      // After resume: running
      const running: ClickHouseEtlStatus = {
        state: "running",
        batches_done: 200,
        batches_total: 400,
        last_heartbeat_at: new Date().toISOString(),
        finished_at: null,
        error: null,
      };

      let resumeCallCount = 0;
      let statusCallCount = 0;

      await page.route("**/v1/setup/clickhouse/status", (route) => {
        // First call returns interrupted; subsequent calls (after resume) return running
        const payload = statusCallCount === 0 ? interrupted : running;
        statusCallCount += 1;
        void route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify(payload),
        });
      });

      await page.route("**/v1/setup/clickhouse/resume", (route) => {
        resumeCallCount += 1;
        void route.fulfill({
          status: 202,
          contentType: "application/json",
          body: JSON.stringify({ ok: true }),
        });
      });

      await page.goto("/setup");

      const resumeBtn = page.getByTestId("etl-resume-button");
      await expect(resumeBtn).toBeVisible();

      // Click resume
      await resumeBtn.click();

      // POST must have been called
      expect(resumeCallCount, "POST /v1/setup/clickhouse/resume should be called once").toBe(1);

      // After router.refresh(), the badge should show "running"
      const badge = page.getByTestId("etl-state-badge");
      await expect(badge).toContainText("running");

      // Resume button must disappear in "running" state
      await expect(page.getByTestId("etl-resume-button")).toHaveCount(0);
    },
  );
});
