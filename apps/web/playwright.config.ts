import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "./tests/e2e",
  // Exclude developer/ops specs that require an external running stack
  // (dashboard + API + seeded data). They opt in via
  // PLAYWRIGHT_INCLUDE_CAPTURE=1 — see apps/web/tests/e2e/capture-screenshots.spec.ts.
  testIgnore: process.env.PLAYWRIGHT_INCLUDE_CAPTURE
    ? undefined
    : ["**/capture-screenshots.spec.ts"],
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 4 : undefined,
  reporter: process.env.CI
    ? [["html", { open: "never" }], ["github"], ["list"]]
    : [["list"], ["html", { open: "never" }]],
  timeout: 30_000,
  expect: { timeout: 5_000 },
  use: {
    baseURL: process.env.PLAYWRIGHT_BASE_URL ?? "http://localhost:3000",
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
    video: "retain-on-failure",
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
  webServer: process.env.PLAYWRIGHT_SKIP_WEBSERVER
    ? undefined
    : [
        // (1) Lightweight mock API backend. The dashboard pages SSR-fetch the
        // OpenGEO API; Playwright page.route() can't intercept server-side
        // fetches, so we run a canned-JSON backend and point the dev server's
        // OGEO_API_BASE_URL at it (below) so SSR returns spec-satisfying data.
        {
          command: "node tests/e2e/mock-api-server.mjs",
          url: "http://127.0.0.1:8787/healthz",
          reuseExistingServer: !process.env.CI,
          timeout: 30_000,
        },
        // (2) The Next.js dev server, with its API base pointed at the mock.
        {
          command: "pnpm dev",
          url: "http://localhost:3000",
          reuseExistingServer: !process.env.CI,
          timeout: 120_000,
          env: {
            // 127.0.0.1 (not localhost): on dual-stack CI runners undici would
            // resolve `localhost` to ::1 first and miss the IPv4 mock backend.
            OGEO_API_BASE_URL: "http://127.0.0.1:8787",
            OGEO_API_KEY: "e2e-test-key",
          },
        },
      ],
});
