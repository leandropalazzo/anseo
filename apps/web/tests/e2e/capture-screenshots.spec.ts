// One-off capture script for docs/images/manual/*.png.
//
// Boots against `pnpm dev` (or a running app) and screenshots each surface.
// Most surfaces render from baked-in mock data (lib/mock*.ts), so no API /
// Postgres / ClickHouse is required.
//
//   PLAYWRIGHT_INCLUDE_CAPTURE=1 \
//   PLAYWRIGHT_BASE_URL=http://localhost:3000 \
//     pnpm exec playwright test capture-screenshots.spec.ts \
//       --reporter=line --workers=1

import { test } from "@playwright/test";
import path from "node:path";

const OUTPUT_DIR =
  process.env.OUTPUT_DIR ??
  path.resolve(__dirname, "../../../..", "docs/images/manual");

const SIDEBAR = 'nav[aria-label="Primary"]';

type Shot = { path: string; file: string; theme?: "light" | "dark" };

const SHOTS: ReadonlyArray<Shot> = [
  { path: "/", file: "overview.png" },
  { path: "/", file: "overview-light.png", theme: "light" },
  { path: "/runs", file: "runs-list.png" },
  { path: "/visibility", file: "visibility.png" },
  { path: "/citations", file: "citations.png" },
  { path: "/competitors", file: "competitors.png" },
  { path: "/prompts", file: "prompts.png" },
  { path: "/recommendations", file: "recommendations.png" },
  { path: "/alerts", file: "alerts.png" },
  { path: "/mcp", file: "mcp.png" },
  { path: "/marketplace", file: "marketplace.png" },
  { path: "/settings", file: "settings.png" },
];

test.use({ viewport: { width: 1440, height: 900 } });
test.describe.configure({ timeout: 120_000 });

for (const shot of SHOTS) {
  test(`capture ${shot.file}`, async ({ page }) => {
    const theme = shot.theme ?? "dark";
    await page.addInitScript((t) => {
      try {
        window.localStorage.setItem("ogeo-theme", t);
      } catch {
        /* ignore */
      }
    }, theme);
    await page.goto(shot.path, { waitUntil: "networkidle", timeout: 90_000 });
    await page.locator(SIDEBAR).waitFor({ state: "visible", timeout: 90_000 });
    // Settle so fonts/charts finish painting before the capture.
    await page.waitForTimeout(600);
    await page.screenshot({
      path: path.join(OUTPUT_DIR, shot.file),
      fullPage: true,
    });
  });
}
