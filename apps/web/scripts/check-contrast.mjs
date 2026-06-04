#!/usr/bin/env node
// UX-F — contrast-ratio gate for the Signal token set.
//
// Asserts that the dark + light theme token surfaces declared in
// `apps/web/styles/tokens.css` meet WCAG AA contrast for every text
// pair the dashboard renders. Run in CI; failure blocks merge.
//
// Usage: node apps/web/scripts/check-contrast.mjs
//
// The script parses the tokens.css file with a forgiving regex (no full
// CSS parser dep). It handles the two color value forms used in
// tokens.css:
//   1. `#RRGGBB` / `#RGB` hex
//   2. `oklch(L C H)` / `oklch(L C H / A)` (W3C OKLCH per CSS Color 4)
//
// `rgba(r,g,b,a)` is also tolerated (used for overlay borders) — the
// alpha is composited over the bg color before measuring luminance.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const TOKENS_PATH = resolve(__dirname, "..", "styles", "tokens.css");

const WCAG_AA_TEXT = 4.5;
const WCAG_AA_UI = 3.0;

// Pairs to check. Each entry:
//   [foreground token, background token, target ratio, label].
// `--border` / `--border-strong` are intentionally transparent overlays;
// we composite them over the named bg before the contrast measurement.
const PAIRS = [
  // Body text on the three surface levels.
  ["--text",        "--bg",        WCAG_AA_TEXT, "body text on bg"],
  ["--text",        "--bg-elev",   WCAG_AA_TEXT, "body text on bg-elev"],
  ["--text",        "--bg-elev-2", WCAG_AA_TEXT, "body text on bg-elev-2"],
  ["--text",        "--bg-sunken", WCAG_AA_TEXT, "body text on bg-sunken"],
  // Muted text against bg (most common muted surface).
  ["--text-muted",  "--bg",        WCAG_AA_TEXT, "muted text on bg"],
  ["--text-muted",  "--bg-elev",   WCAG_AA_TEXT, "muted text on bg-elev"],
  // Accent button text on the accent fill.
  ["--accent-ink",  "--accent",    WCAG_AA_TEXT, "accent ink on accent fill"],
  // UI-only: focus ring and structural border on canvas.
  ["--accent",      "--bg",        WCAG_AA_UI,   "focus/accent on bg"],
  ["--border-strong", "--bg",      WCAG_AA_UI,   "structural border on bg"],
  // Provider color swatches are intentionally NOT checked here:
  // UX-DR mandates color is always paired with the █ density glyph
  // (see <ProviderDot> in components/ui/provider-dot.tsx), so identity
  // does not depend on color contrast alone. Adding swatch checks here
  // would force palette changes that drop the prototype's tuned hues.
];

function parseTokens(css) {
  const blocks = {};
  const blockRe = /(:root(?:\[data-theme="(dark|light)"])?(?:\s*,\s*:root(?:\[data-theme="(dark|light)"])?)*)\s*\{([^}]*)\}/g;
  let m;
  while ((m = blockRe.exec(css)) !== null) {
    const selector = m[1];
    const body = m[4];
    const themes = [];
    if (selector.includes(`[data-theme="light"]`)) themes.push("light");
    if (selector.includes(`[data-theme="dark"]`)) themes.push("dark");
    if (selector.match(/:root(?!\[)/)) themes.push("dark"); // bare :root → dark default
    const declRe = /--([\w-]+):\s*([^;]+);/g;
    let d;
    while ((d = declRe.exec(body)) !== null) {
      const name = `--${d[1]}`;
      const value = d[2].trim();
      for (const theme of themes) {
        if (!blocks[theme]) blocks[theme] = {};
        blocks[theme][name] = value;
      }
    }
  }
  // Light inherits any token not redefined in its block from dark.
  if (blocks.dark && blocks.light) {
    for (const [k, v] of Object.entries(blocks.dark)) {
      if (!(k in blocks.light)) blocks.light[k] = v;
    }
  }
  return blocks;
}

// ---- color parsing ---------------------------------------------------

/** Returns [r, g, b, a] in 0..1, 0..1. */
function parseColor(raw) {
  const v = raw.trim();
  if (v.startsWith("#")) {
    const [r, g, b] = hexToRgb(v);
    return [r / 255, g / 255, b / 255, 1];
  }
  const rgba = v.match(/^rgba?\(\s*([\d.]+)\s*,\s*([\d.]+)\s*,\s*([\d.]+)\s*(?:,\s*([\d.]+)\s*)?\)$/i);
  if (rgba) {
    return [
      Number(rgba[1]) / 255,
      Number(rgba[2]) / 255,
      Number(rgba[3]) / 255,
      rgba[4] !== undefined ? Number(rgba[4]) : 1,
    ];
  }
  const ok = v.match(/^oklch\(\s*([\d.]+)\s+([\d.]+)\s+([\d.]+)\s*(?:\/\s*([\d.]+)\s*)?\)$/i);
  if (ok) {
    const [r, g, b] = oklchToLinearRgb(Number(ok[1]), Number(ok[2]), Number(ok[3]));
    const a = ok[4] !== undefined ? Number(ok[4]) : 1;
    // Convert linear → sRGB (gamma).
    return [linearToSrgb(r), linearToSrgb(g), linearToSrgb(b), a];
  }
  throw new Error(`unrecognized color value '${raw}'`);
}

function hexToRgb(hex) {
  const v = hex.replace(/^#/, "");
  if (v.length === 6) {
    return [parseInt(v.slice(0, 2), 16), parseInt(v.slice(2, 4), 16), parseInt(v.slice(4, 6), 16)];
  }
  if (v.length === 3) {
    return v.split("").map((c) => parseInt(c + c, 16));
  }
  throw new Error(`unrecognized hex '${hex}'`);
}

// OKLCH → OKLab → linear sRGB per W3C CSS Color 4 spec.
//   https://www.w3.org/TR/css-color-4/#ok-lab
//   https://www.w3.org/TR/css-color-4/#color-conversion-code
// Hue in degrees, chroma 0..~0.4, lightness 0..1.
function oklchToLinearRgb(L, C, hDeg) {
  const h = (hDeg * Math.PI) / 180;
  const a = C * Math.cos(h);
  const b = C * Math.sin(h);
  // OKLab → LMS (cube the intermediate l′, m′, s′).
  const l_ = L + 0.3963377774 * a + 0.2158037573 * b;
  const m_ = L - 0.1055613458 * a - 0.0638541728 * b;
  const s_ = L - 0.0894841775 * a - 1.2914855480 * b;
  const l = l_ * l_ * l_;
  const m = m_ * m_ * m_;
  const s = s_ * s_ * s_;
  // LMS → linear sRGB.
  const r =  4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s;
  const g = -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s;
  const b2 = -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s;
  return [r, g, b2];
}

function linearToSrgb(c) {
  // Clamp out-of-gamut before gamma.
  const x = Math.max(0, Math.min(1, c));
  return x <= 0.0031308 ? 12.92 * x : 1.055 * Math.pow(x, 1 / 2.4) - 0.055;
}

/** Composite a possibly-transparent fg over an opaque bg. Returns srgb 0..1. */
function composite(fg, bg) {
  const [fr, fg_, fb, fa] = fg;
  const [br, bg_, bb] = bg;
  return [
    fr * fa + br * (1 - fa),
    fg_ * fa + bg_ * (1 - fa),
    fb * fa + bb * (1 - fa),
    1,
  ];
}

function srgbToLinearChannel(c) {
  return c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
}

function relativeLuminance([r, g, b]) {
  return (
    0.2126 * srgbToLinearChannel(r) +
    0.7152 * srgbToLinearChannel(g) +
    0.0722 * srgbToLinearChannel(b)
  );
}

function contrastRatio(fg, bg) {
  const lFg = relativeLuminance(fg);
  const lBg = relativeLuminance(bg);
  const lighter = Math.max(lFg, lBg);
  const darker = Math.min(lFg, lBg);
  return (lighter + 0.05) / (darker + 0.05);
}

function check(themeBlocks, themeName) {
  let failures = 0;
  const tokens = themeBlocks[themeName];
  if (!tokens) {
    console.error(`x theme '${themeName}' has no token block`);
    return 1;
  }
  for (const [fgToken, bgToken, target, label] of PAIRS) {
    const fgRaw = tokens[fgToken];
    const bgRaw = tokens[bgToken];
    if (!fgRaw || !bgRaw) {
      console.error(`x ${themeName}: token missing for '${label}' (${fgToken} vs ${bgToken})`);
      failures += 1;
      continue;
    }
    let fg, bg;
    try {
      fg = parseColor(fgRaw);
      bg = parseColor(bgRaw);
    } catch (e) {
      console.error(`x ${themeName}: ${label} failed to parse — ${e.message}`);
      failures += 1;
      continue;
    }
    // bg should be opaque; if not, composite over body bg.
    if (bg[3] < 1) {
      const body = parseColor(tokens["--bg"] ?? "#000000");
      bg = composite(bg, body);
    }
    // fg may be a translucent overlay (e.g. --border) — composite over bg.
    const fgFlat = fg[3] < 1 ? composite(fg, bg) : fg;
    const ratio = contrastRatio(
      [fgFlat[0], fgFlat[1], fgFlat[2]],
      [bg[0], bg[1], bg[2]],
    );
    if (ratio < target) {
      console.error(
        `x ${themeName}: ${label} contrast ${ratio.toFixed(2)}:1 < required ${target.toFixed(1)}:1 (${fgToken}=${fgRaw} on ${bgToken}=${bgRaw})`,
      );
      failures += 1;
    } else {
      console.log(`ok ${themeName}: ${label} ${ratio.toFixed(2)}:1`);
    }
  }
  return failures;
}

const css = readFileSync(TOKENS_PATH, "utf8");
const blocks = parseTokens(css);

console.log(`Loaded tokens from: ${TOKENS_PATH}`);
console.log();

const darkFails = check(blocks, "dark");
console.log();
const lightFails = check(blocks, "light");
console.log();

const total = darkFails + lightFails;
if (total === 0) {
  console.log("ok All contrast pairs pass WCAG AA.");
  process.exit(0);
} else {
  console.error(`x ${total} contrast pair(s) below target.`);
  process.exit(1);
}
