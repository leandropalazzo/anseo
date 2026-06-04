// Story 15.6 — API-key inventory table and webhook target test sections.
//
// Exercises the upgraded ApiKeysCard (table + revoke) and the new
// WebhookTargetCard (URL input + test + result panel). All API calls
// are intercepted via page.route() so no live backend is required.
//
// trace: Story-15.6 AC-1 (api-keys table), AC-2 (webhook test), AC-3 (axe), AC-4 (visual walkthrough)

import { test, expect } from "./fixtures";

// ─── Mock data ───────────────────────────────────────────────────────────────

const MOCK_STATUS = {
  postgres: {
    state: "healthy",
    schema_version: 42,
    row_count_estimate: 1234,
    last_write_at: "2026-05-01T12:00:00Z",
  },
  clickhouse: {
    state: "not_configured",
    url: null,
    row_count: null,
    etl_lag_seconds: null,
  },
  worker: {
    state: "running",
    uptime_seconds: 3600,
    queue_depth: 0,
  },
  webhook_target: {
    configured: true,
    last_delivery_at: "2026-05-01T11:00:00Z",
    last_status: "200",
  },
  api_keys: [
    { provider: "openai", configured: true, last_used_at: "2026-05-01T10:00:00Z" },
    { provider: "anthropic", configured: true, last_used_at: null },
    { provider: "google", configured: false, last_used_at: null },
  ],
  docker: { present: true, version: "24.0.0" },
};

// The "with keys" default. Empty-state coverage is driven through SSR via
// `/setup?empty=1` (the mock backend serves its empty variant for that flag),
// since page.route() can't intercept server-side fetches.
async function mockSetupStatus(
  page: import("@playwright/test").Page,
  body: typeof MOCK_STATUS,
) {
  await page.route("**/v1/setup/status", (route) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify(body),
    }),
  );
}

// ─── API keys table ───────────────────────────────────────────────────────────

test("@P1 api keys table renders provider rows", async ({ page }) => {
  await mockSetupStatus(page, MOCK_STATUS);
  await page.goto("/setup");
  await page.waitForLoadState("networkidle");

  const table = page.getByTestId("api-keys-table");
  await expect(table).toBeVisible();

  await expect(page.getByTestId("api-key-row-openai")).toBeVisible();
  await expect(page.getByTestId("api-key-row-anthropic")).toBeVisible();
  await expect(page.getByTestId("api-key-row-google")).toBeVisible();
});

test("@P1 api key revoke button visible per row", async ({ page }) => {
  await mockSetupStatus(page, MOCK_STATUS);
  await page.goto("/setup");
  await page.waitForLoadState("networkidle");

  // Configured providers should have an enabled Revoke button
  await expect(page.getByTestId("api-key-revoke-openai")).toBeVisible();
  await expect(page.getByTestId("api-key-revoke-openai")).toBeEnabled();

  await expect(page.getByTestId("api-key-revoke-anthropic")).toBeVisible();
  await expect(page.getByTestId("api-key-revoke-anthropic")).toBeEnabled();

  // Not-configured provider should have a disabled Revoke button
  await expect(page.getByTestId("api-key-revoke-google")).toBeVisible();
  await expect(page.getByTestId("api-key-revoke-google")).toBeDisabled();
});

test("@P1 api key revoke marks key as not configured", async ({ page }) => {
  await mockSetupStatus(page, MOCK_STATUS);

  // Mock the revoke endpoint
  await page.route("**/v1/setup/api-keys/openai/revoke", (route) =>
    route.fulfill({ status: 204, body: "" }),
  );

  await page.goto("/setup");
  await page.waitForLoadState("networkidle");

  // Revoke openai
  await page.getByTestId("api-key-revoke-openai").click();

  // After revoke, the openai key should be reflected as not configured
  // (row still present, revoke button now disabled)
  await expect(page.getByTestId("api-key-row-openai")).toBeVisible();
  await expect(page.getByTestId("api-key-revoke-openai")).toBeDisabled();
});

test("@P1 api keys table empty state", async ({ page }) => {
  // The setup page SSR-fetches /v1/setup/status, which page.route() can't
  // intercept; `?empty=1` forwards to the mock backend's empty variant.
  await page.goto("/setup?empty=1");
  await page.waitForLoadState("networkidle");

  const table = page.getByTestId("api-keys-table");
  await expect(table).toBeVisible();
  await expect(table).toContainText("No API keys configured");
});

// ─── Webhook target card ──────────────────────────────────────────────────────

test("@P1 webhook test happy path shows signature valid", async ({ page }) => {
  await mockSetupStatus(page, MOCK_STATUS);

  await page.route("**/v1/setup/webhook/test", (route) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        status_code: 200,
        signature_valid: true,
        latency_ms: 42,
        error: null,
      }),
    }),
  );

  await page.goto("/setup");
  await page.waitForLoadState("networkidle");

  await page.getByTestId("webhook-url-input").fill("https://example.com/webhook");
  await page.getByTestId("webhook-test-button").click();

  const result = page.getByTestId("webhook-test-result");
  await expect(result).toBeVisible();

  const sigStatus = page.getByTestId("webhook-signature-status");
  await expect(sigStatus).toBeVisible();
  await expect(sigStatus).toContainText("true");
});

test("@P1 webhook test bad URL shows error", async ({ page }) => {
  await mockSetupStatus(page, MOCK_STATUS);

  await page.route("**/v1/setup/webhook/test", (route) =>
    route.fulfill({
      status: 400,
      contentType: "application/json",
      body: JSON.stringify({ error: "invalid URL" }),
    }),
  );

  await page.goto("/setup");
  await page.waitForLoadState("networkidle");

  await page.getByTestId("webhook-url-input").fill("https://bad-url.example.com/hook");
  await page.getByTestId("webhook-test-button").click();

  const result = page.getByTestId("webhook-test-result");
  await expect(result).toBeVisible();
  // When status >= 400, the fetch wrapper returns an error object
  await expect(result).toContainText(/bad request|error/i);
});

test("@P1 webhook test signature failure shows false", async ({ page }) => {
  await mockSetupStatus(page, MOCK_STATUS);

  await page.route("**/v1/setup/webhook/test", (route) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        status_code: 200,
        signature_valid: false,
        latency_ms: 88,
        error: null,
      }),
    }),
  );

  await page.goto("/setup");
  await page.waitForLoadState("networkidle");

  await page.getByTestId("webhook-url-input").fill("https://example.com/webhook");
  await page.getByTestId("webhook-test-button").click();

  const sigStatus = page.getByTestId("webhook-signature-status");
  await expect(sigStatus).toBeVisible();
  await expect(sigStatus).toContainText("false");
});

test("@P1 webhook test button disabled while request in flight", async ({ page }) => {
  await mockSetupStatus(page, MOCK_STATUS);

  // Slow route to capture the loading state
  await page.route("**/v1/setup/webhook/test", async (route) => {
    await new Promise((r) => setTimeout(r, 300));
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        status_code: 200,
        signature_valid: true,
        latency_ms: 300,
        error: null,
      }),
    });
  });

  await page.goto("/setup");
  await page.waitForLoadState("networkidle");

  await page.getByTestId("webhook-url-input").fill("https://example.com/webhook");

  const button = page.getByTestId("webhook-test-button");
  await button.click();

  // During the 300ms delay the button should be disabled
  await expect(button).toBeDisabled();

  // After the response the result should appear
  await expect(page.getByTestId("webhook-test-result")).toBeVisible();
});

// ─── Axe accessibility ────────────────────────────────────────────────────────

test("@P0 @A11y setup/api-keys default state passes WCAG 2.1 AA", async ({
  page,
  axe,
}) => {
  await mockSetupStatus(page, MOCK_STATUS);
  await page.goto("/setup");
  await page.waitForLoadState("networkidle");
  await axe('[data-testid="api-keys-table"]');
});

test("@P0 @A11y setup/api-keys empty state passes WCAG 2.1 AA", async ({
  page,
  axe,
}) => {
  await page.goto("/setup?empty=1");
  await page.waitForLoadState("networkidle");
  await axe('[data-testid="api-keys-table"]');
});

test("@P0 @A11y webhook-target-card default state passes WCAG 2.1 AA", async ({
  page,
  axe,
}) => {
  await mockSetupStatus(page, MOCK_STATUS);
  await page.goto("/setup");
  await page.waitForLoadState("networkidle");
  await axe('[data-testid="webhook-target-card"]');
});

test("@P0 @A11y webhook-target-card result panel passes WCAG 2.1 AA", async ({
  page,
  axe,
}) => {
  await mockSetupStatus(page, MOCK_STATUS);

  await page.route("**/v1/setup/webhook/test", (route) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        status_code: 200,
        signature_valid: true,
        latency_ms: 12,
        error: null,
      }),
    }),
  );

  await page.goto("/setup");
  await page.waitForLoadState("networkidle");
  await page.getByTestId("webhook-url-input").fill("https://example.com/webhook");
  await page.getByTestId("webhook-test-button").click();
  await expect(page.getByTestId("webhook-test-result")).toBeVisible();

  await axe('[data-testid="webhook-target-card"]');
});

test("@P0 @A11y webhook-target-card empty/unconfigured state passes WCAG 2.1 AA", async ({
  page,
  axe,
}) => {
  await page.goto("/setup?empty=1");
  await page.waitForLoadState("networkidle");
  await axe('[data-testid="webhook-target-card"]');
});
