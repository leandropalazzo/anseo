// Story 16.x — MCP dashboard smoke + functional tests.
//
// Exercises the /mcp page: ToolBrowser renders tool list, ActivityLog shows
// empty state when no calls are recorded, and the page passes WCAG 2.1 AA
// in both light and dark themes.
//
// All API calls are intercepted via page.route() so no live backend is
// required.
//
// trace: AC-1 (tool list renders), AC-2 (empty activity log state), AC-3 (axe light), AC-4 (axe dark)

import { test, expect } from "./fixtures";
import type { McpToolInfo, McpCallRow } from "../../lib/api";

// ─── Mock data ───────────────────────────────────────────────────────────────

const MOCK_TOOLS: McpToolInfo[] = [
  {
    id: "get_visibility",
    sig: "(prompt: string, days?: number)",
    doc: "Get visibility scores for a prompt",
    category: "visibility",
  },
  {
    id: "run_prompt",
    sig: "(prompt: string)",
    doc: "Run a prompt across configured providers",
    category: "runs",
  },
];

const MOCK_CALLS: McpCallRow[] = [];

async function mockMcpEndpoints(page: import("@playwright/test").Page): Promise<void> {
  await page.route("**/v1/mcp/tools", (route) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ tools: MOCK_TOOLS }),
    }),
  );
  await page.route("**/v1/mcp/calls**", (route) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ calls: MOCK_CALLS }),
    }),
  );
}

// ─── Smoke: page renders with tools ──────────────────────────────────────────

test("@P1 mcp page renders with tools", async ({ page }) => {
  await mockMcpEndpoints(page);
  await page.goto("/mcp");
  await page.waitForLoadState("networkidle");

  await expect(page.getByTestId("mcp-page")).toBeVisible();

  // ToolBrowser renders each tool id as a button (data-testid="mcp-tool-<id>")
  await expect(page.getByTestId("mcp-tool-get_visibility")).toBeVisible();
  await expect(page.getByTestId("mcp-tool-run_prompt")).toBeVisible();
});

// ─── Empty activity log ───────────────────────────────────────────────────────

test("@P1 empty activity log shows empty state", async ({ page }) => {
  await mockMcpEndpoints(page);
  await page.goto("/mcp");
  await page.waitForLoadState("networkidle");

  // ActivityLog empty state shows its unique copy. (Scope to this sentence —
  // "ogeo mcp serve" alone now also matches the ToolBrowser setup guide's
  // CodeBlock, tripping Playwright strict mode.)
  await expect(page.getByText("No tool calls recorded yet")).toBeVisible();
});

// ─── Axe accessibility ────────────────────────────────────────────────────────

test("@P0 @A11y mcp page passes WCAG 2.1 AA in light theme", async ({
  page,
  axe,
}) => {
  await mockMcpEndpoints(page);

  await page.addInitScript((t) => {
    try {
      window.localStorage.setItem("ogeo-theme", t);
    } catch {
      /* sandboxed contexts may forbid localStorage */
    }
  }, "light");

  await page.goto("/mcp");

  await page.evaluate((t) => {
    document.documentElement.setAttribute("data-theme", t);
  }, "light");

  await expect(page.getByTestId("mcp-page")).toBeVisible();
  await axe();
});

test("@P0 @A11y mcp page passes WCAG 2.1 AA in dark theme", async ({
  page,
  axe,
}) => {
  await mockMcpEndpoints(page);

  await page.addInitScript((t) => {
    try {
      window.localStorage.setItem("ogeo-theme", t);
    } catch {
      /* sandboxed contexts may forbid localStorage */
    }
  }, "dark");

  await page.goto("/mcp");

  await page.evaluate((t) => {
    document.documentElement.setAttribute("data-theme", t);
  }, "dark");

  await expect(page.getByTestId("mcp-page")).toBeVisible();
  await axe();
});
