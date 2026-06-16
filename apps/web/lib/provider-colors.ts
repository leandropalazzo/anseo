/**
 * Provider color identity — CONSTANT across themes.
 *
 * A11y rule (UX-DR): components MUST pair the color with the `█` density
 * glyph so colorblind operators have a non-color signal. The
 * `<ProviderDot>` primitive does this automatically; ad-hoc usages should
 * follow the same pattern.
 *
 * The `var` field references CSS variables declared in
 * `apps/web/styles/tokens.css`.
 */

export type ProviderId =
  | "openai"
  | "anthropic"
  | "gemini"
  | "perplexity"
  | "grok"
  | "mistral"
  | "openrouter";

export type ConcreteProviderId = Exclude<ProviderId, "openrouter">;

export const CONCRETE_PROVIDER_IDS: ReadonlyArray<ConcreteProviderId> = [
  "openai",
  "anthropic",
  "gemini",
  "perplexity",
  "grok",
  "mistral",
];

export const CREDENTIAL_PROVIDER_IDS: ReadonlyArray<ProviderId> = [
  ...CONCRETE_PROVIDER_IDS,
  "openrouter",
];

export interface ProviderColor {
  /** CSS custom property name, e.g. `var(--p-openai)`. */
  readonly var: `--p-${ProviderId}`;
  /** Density glyph paired with the color for non-color a11y signal. */
  readonly glyph: "█";
  /** Display label. */
  readonly label: string;
  /** Compact logo-style mark used inside provider chips. */
  readonly logo: string;
  /** Optional monochrome brand icon path, rendered inside the color chip. */
  readonly iconPath?: string;
}

export const PROVIDER_COLORS: Readonly<Record<ProviderId, ProviderColor>> = {
  openai: {
    var: "--p-openai",
    glyph: "█",
    label: "OpenAI",
    logo: "O",
    iconPath:
      "M22.2819 9.8211a5.9847 5.9847 0 0 0-.5157-4.9108 6.0462 6.0462 0 0 0-6.5098-2.9A6.0651 6.0651 0 0 0 4.9807 4.1818a5.9847 5.9847 0 0 0-3.9977 2.9 6.0462 6.0462 0 0 0 .7427 7.0966 5.98 5.98 0 0 0 .511 4.9107 6.051 6.051 0 0 0 6.5146 2.9001A5.9847 5.9847 0 0 0 13.2599 24a6.0557 6.0557 0 0 0 5.7718-4.2058 5.9894 5.9894 0 0 0 3.9977-2.9001 6.0557 6.0557 0 0 0-.7475-7.0729zm-9.022 12.6081a4.4755 4.4755 0 0 1-2.8764-1.0408l.1419-.0804 4.7783-2.7582a.7948.7948 0 0 0 .3927-.6813v-6.7369l2.02 1.1686a.071.071 0 0 1 .038.052v5.5826a4.504 4.504 0 0 1-4.4945 4.4944zm-9.6607-4.1254a4.4708 4.4708 0 0 1-.5346-3.0137l.142.0852 4.783 2.7582a.7712.7712 0 0 0 .7806 0l5.8428-3.3685v2.3324a.0804.0804 0 0 1-.0332.0615L9.74 19.9502a4.4992 4.4992 0 0 1-6.1408-1.6464zM2.3408 7.8956a4.485 4.485 0 0 1 2.3655-1.9728V11.6a.7664.7664 0 0 0 .3879.6765l5.8144 3.3543-2.0201 1.1685a.0757.0757 0 0 1-.071 0l-4.8303-2.7865A4.504 4.504 0 0 1 2.3408 7.872zm16.5963 3.8558L13.1038 8.364 15.1192 7.2a.0757.0757 0 0 1 .071 0l4.8303 2.7913a4.4944 4.4944 0 0 1-.6765 8.1042v-5.6772a.79.79 0 0 0-.407-.667zm2.0107-3.0231-.142-.0852-4.7735-2.7818a.7759.7759 0 0 0-.7854 0L9.409 9.2297V6.8974a.0662.0662 0 0 1 .0284-.0615l4.8303-2.7866a4.4992 4.4992 0 0 1 6.6802 4.66zM8.3065 12.863l-2.02-1.1638a.0804.0804 0 0 1-.038-.0567V6.0742a4.4992 4.4992 0 0 1 7.3757-3.4537l-.142.0805L8.704 5.459a.7948.7948 0 0 0-.3927.6813zm1.0976-2.3654 2.602-1.4998 2.6069 1.4998v2.9994l-2.5974 1.4997-2.6067-1.4997Z",
  },
  anthropic: {
    var: "--p-anthropic",
    glyph: "█",
    label: "Anthropic",
    logo: "A",
    iconPath:
      "M17.3041 3.541h-3.6718l6.696 16.918H24Zm-10.6082 0L0 20.459h3.7442l1.3693-3.5527h7.0052l1.3693 3.5528h3.7442L10.5363 3.5409Zm-.3712 10.2232 2.2914-5.9456 2.2914 5.9456Z",
  },
  gemini: {
    var: "--p-gemini",
    glyph: "█",
    label: "Gemini",
    logo: "G",
    iconPath:
      "M11.04 19.32Q12 21.51 12 24q0-2.49.93-4.68.96-2.19 2.58-3.81t3.81-2.55Q21.51 12 24 12q-2.49 0-4.68-.93a12.3 12.3 0 0 1-3.81-2.58 12.3 12.3 0 0 1-2.58-3.81Q12 2.49 12 0q0 2.49-.96 4.68-.93 2.19-2.55 3.81a12.3 12.3 0 0 1-3.81 2.58Q2.49 12 0 12q2.49 0 4.68.96 2.19.93 3.81 2.55t2.55 3.81",
  },
  perplexity: {
    var: "--p-perplexity",
    glyph: "█",
    label: "Perplexity",
    logo: "P",
    iconPath:
      "M22.3977 7.0896h-2.3106V.0676l-7.5094 6.3542V.1577h-1.1554v6.1966L4.4904 0v7.0896H1.6023v10.3976h2.8882V24l6.932-6.3591v6.2005h1.1554v-6.0469l6.9318 6.1807v-6.4879h2.8882V7.0896zm-3.4657-4.531v4.531h-5.355l5.355-4.531zm-13.2862.0676 4.8691 4.4634H5.6458V2.6262zM2.7576 16.332V8.245h7.8476l-6.1149 6.1147v1.9723H2.7576zm2.8882 5.0404v-3.8852h.0001v-2.6488l5.7763-5.7764v7.0111l-5.7764 5.2993zm12.7086.0248-5.7766-5.1509V9.0618l5.7766 5.7766v6.5588zm2.8882-5.0652h-1.733v-1.9723L13.3948 8.245h7.8478v8.087z",
  },
  grok: {
    var: "--p-grok",
    glyph: "█",
    label: "Grok",
    logo: "x",
    iconPath:
      "M14.234 10.162 22.977 0h-2.072l-7.591 8.824L7.251 0H.258l9.168 13.343L.258 24H2.33l8.016-9.318L16.749 24h6.993zm-2.837 3.299-.929-1.329L3.076 1.56h3.182l5.965 8.532.929 1.329 7.754 11.09h-3.182z",
  },
  mistral: {
    var: "--p-mistral",
    glyph: "█",
    label: "Mistral",
    logo: "M",
    iconPath:
      "M17.143 3.429v3.428h-3.429v3.429h-3.428V6.857H6.857V3.43H3.43v13.714H0v3.428h10.286v-3.428H6.857v-3.429h3.429v3.429h3.429v-3.429h3.428v3.429h-3.428v3.428H24v-3.428h-3.43V3.429z",
  },
  openrouter: { var: "--p-openrouter", glyph: "█", label: "OpenRouter", logo: "R" },
};

const DEEPSEEK_ICON_PATH =
  "M23.748 4.651c-.254-.124-.364.113-.512.233-.051.04-.094.09-.137.137-.372.397-.806.657-1.373.626-.829-.046-1.537.214-2.163.848-.133-.782-.575-1.248-1.247-1.548-.352-.155-.708-.311-.955-.65-.172-.24-.219-.509-.305-.774-.055-.16-.11-.323-.293-.35-.2-.031-.278.136-.356.276-.313.572-.434 1.202-.422 1.84.027 1.436.633 2.58 1.838 3.393.137.094.172.187.129.323-.082.28-.18.553-.266.833-.055.179-.137.218-.328.14a5.5 5.5 0 0 1-1.737-1.179c-.857-.828-1.631-1.743-2.597-2.46a12 12 0 0 0-.689-.47c-.985-.957.13-1.743.387-1.836.27-.098.094-.433-.778-.428-.872.003-1.67.295-2.687.685a3 3 0 0 1-.465.136 9.6 9.6 0 0 0-2.883-.101c-1.885.21-3.39 1.1-4.497 2.622C.082 8.776-.231 10.854.152 13.02c.403 2.284 1.568 4.175 3.36 5.653 1.857 1.533 3.997 2.284 6.438 2.14 1.482-.085 3.132-.284 4.994-1.86.47.234.962.328 1.78.398.629.058 1.235-.031 1.705-.129.735-.155.684-.836.418-.961-2.155-1.004-1.682-.595-2.112-.926 1.095-1.295 2.768-3.598 3.284-6.733.05-.346.115-.834.108-1.114-.004-.171.035-.238.23-.257a4.2 4.2 0 0 0 1.545-.475c1.397-.763 1.96-2.016 2.093-3.517.02-.23-.004-.467-.247-.588M11.58 18.168c-2.088-1.642-3.101-2.183-3.52-2.16-.39.024-.32.472-.234.763.09.288.207.487.371.74.114.167.192.416-.113.603-.673.416-1.842-.14-1.897-.168-1.361-.801-2.5-1.86-3.301-3.306-.775-1.393-1.225-2.888-1.299-4.482-.02-.385.094-.522.477-.592a4.7 4.7 0 0 1 1.53-.038c2.131.311 3.946 1.264 5.467 2.774.868.86 1.525 1.887 2.202 2.89.72 1.066 1.494 2.082 2.48 2.915.348.291.626.513.892.677-.802.09-2.14.109-3.055-.615zm1.001-6.44a.306.306 0 0 1 .415-.287.3.3 0 0 1 .113.074.3.3 0 0 1 .086.214c0 .17-.136.307-.308.307a.303.303 0 0 1-.306-.307m3.11 1.596c-.2.081-.4.151-.591.16a1.25 1.25 0 0 1-.798-.254c-.274-.23-.47-.358-.551-.758a1.7 1.7 0 0 1 .015-.588c.07-.327-.007-.537-.238-.727-.188-.156-.426-.199-.689-.199a.6.6 0 0 1-.254-.078.253.253 0 0 1-.114-.358 1 1 0 0 1 .192-.21c.356-.202.767-.136 1.146.016.352.144.618.408 1.001.782.392.451.462.576.685.915.176.264.336.536.446.848.066.194-.02.353-.25.45";

/** Reads the CSS var as a `var(...)` string for inline `style` consumers. */
export function providerCssVar(provider: ProviderId): string {
  return `var(${PROVIDER_COLORS[provider].var})`;
}

/** Neutral CSS var used when an identity cannot be mapped to a palette. */
const UNKNOWN_VAR = "--p-unknown";

/**
 * Resolved render identity for an arbitrary provider string. Always returns
 * a usable color + glyph + label so callers never crash or render blank.
 */
export interface ResolvedProviderIdentity {
  /** `var(...)` string ready for inline `style` / SVG fill. */
  readonly cssVar: string;
  /** A11y glyph (UX-DR) — pair the color with this. */
  readonly glyph: "█";
  /** Human-readable label. */
  readonly label: string;
  /** Compact logo-style mark. */
  readonly logo: string;
  readonly iconPath?: string;
}

/** Maps an OpenRouter upstream vendor segment to a concrete provider palette. */
function vendorToProviderId(vendor: string): ConcreteProviderId | null {
  switch (vendor) {
    case "openai":
      return "openai";
    case "anthropic":
      return "anthropic";
    // OpenRouter names Gemini upstreams under the `google/` vendor.
    case "google":
    case "gemini":
      return "gemini";
    case "perplexity":
      return "perplexity";
    case "grok":
    case "x-ai":
      return "grok";
    case "mistral":
    case "mistralai":
      return "mistral";
    default:
      return null;
  }
}

function titleCaseProviderSegment(value: string): string {
  return value
    .split(/[-_./\s]+/)
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(" ");
}

function routedUpstreamIdentity(vendor: string): ResolvedProviderIdentity | null {
  switch (vendor) {
    case "deepseek":
      return {
        cssVar: "var(--p-deepseek)",
        glyph: "█",
        label: "DeepSeek",
        logo: "D",
        iconPath: DEEPSEEK_ICON_PATH,
      };
    default:
      return null;
  }
}

/**
 * Tolerant provider-identity resolver. Accepts:
 *  - a plain known provider wire name (`openai`, `anthropic`, `gemini`,
 *    `perplexity`, `grok`, `mistral`, `openrouter`) → its palette entry.
 *  - an OpenRouter identity `openrouter:<upstream_model>` (e.g.
 *    `openrouter:openai/gpt-4o-2024-08-06`) → colored and labelled as the
 *    upstream provider (`google` → Gemini), never as OpenRouter.
 *  - anything unknown → neutral fallback color + the raw id as label.
 *
 * Always pairs the color with the `█` glyph (UX-DR a11y rule).
 */
export function resolveProviderIdentity(id: string): ResolvedProviderIdentity {
  const lower = id.trim().toLowerCase();

  // Plain known provider.
  if (lower in PROVIDER_COLORS) {
    const meta = PROVIDER_COLORS[lower as ProviderId];
    return {
      cssVar: `var(${meta.var})`,
      glyph: "█",
      label: meta.label,
      logo: meta.logo,
      iconPath: meta.iconPath,
    };
  }

  const routedPlain = routedUpstreamIdentity(lower);
  if (routedPlain) return routedPlain;

  // OpenRouter identity: `openrouter:<vendor>/<model>` or legacy
  // `openrouter/<vendor>/<model>` display data. Render as the upstream
  // provider so OpenRouter stays a credential route, not an analytics row.
  if (lower.startsWith("openrouter:") || lower.startsWith("openrouter/")) {
    const upstream = lower.startsWith("openrouter:")
      ? id.trim().slice("openrouter:".length)
      : id.trim().slice("openrouter/".length);
    const vendor = upstream.split("/", 1)[0] ?? "";
    const mapped = vendorToProviderId(vendor);
    if (mapped) {
      const meta = PROVIDER_COLORS[mapped];
      return {
        cssVar: `var(${meta.var})`,
        glyph: "█",
        label: meta.label,
        logo: meta.logo,
        iconPath: meta.iconPath,
      };
    }
    const routed = routedUpstreamIdentity(vendor);
    if (routed) return routed;
    return {
      cssVar: `var(${UNKNOWN_VAR})`,
      glyph: "█",
      label: upstream ? titleCaseProviderSegment(vendor || upstream) : "Provider",
      logo: "?",
    };
  }

  // Unknown — never crash or blank.
  return { cssVar: `var(${UNKNOWN_VAR})`, glyph: "█", label: id, logo: "?" };
}

export function isConcreteProviderId(id: string): id is ConcreteProviderId {
  return (CONCRETE_PROVIDER_IDS as ReadonlyArray<string>).includes(id);
}

export function resolveConcreteProviderId(id: string): ConcreteProviderId | null {
  const lower = id.trim().toLowerCase();
  if (isConcreteProviderId(lower)) return lower;
  if (lower.startsWith("openrouter:") || lower.startsWith("openrouter/")) {
    const upstream = lower.startsWith("openrouter:")
      ? lower.slice("openrouter:".length)
      : lower.slice("openrouter/".length);
    const vendor = upstream.split("/", 1)[0] ?? "";
    if (vendor === "deepseek") return null;
    return vendorToProviderId(vendor);
  }
  return null;
}

export function resolveRunFilterProviderId(id: string): string | null {
  const lower = id.trim().toLowerCase();
  if (isConcreteProviderId(lower)) return lower;
  if (routedUpstreamIdentity(lower)) return lower;
  if (lower.startsWith("openrouter:") || lower.startsWith("openrouter/")) {
    const upstream = lower.startsWith("openrouter:")
      ? lower.slice("openrouter:".length)
      : lower.slice("openrouter/".length);
    const vendor = upstream.split("/", 1)[0] ?? "";
    return vendorToProviderId(vendor) ?? (routedUpstreamIdentity(vendor) ? vendor : null);
  }
  return null;
}

export function providerRunIdentity(
  provider: string,
  providerModelVersion: string,
): string {
  const providerWire = provider.trim().toLowerCase();
  const model = providerModelVersion.trim();
  if (providerWire === "openrouter") {
    const routedModel = model.includes("/") ? model : inferOpenRouterModelRoute(model);
    if (routedModel) return `openrouter:${routedModel}`;
  }
  return provider;
}

function inferOpenRouterModelRoute(model: string): string | null {
  const lower = model.trim().toLowerCase();
  if (!lower) return null;
  if (lower.startsWith("gpt-") || lower.startsWith("o1") || lower.startsWith("o3")) {
    return `openai/${model}`;
  }
  if (lower.startsWith("claude-")) return `anthropic/${model}`;
  if (lower.startsWith("gemini-")) return `google/${model}`;
  if (lower.startsWith("sonar-")) return `perplexity/${model}`;
  if (lower.startsWith("grok-")) return `x-ai/${model}`;
  if (lower.startsWith("mistral-") || lower.startsWith("open-mistral-")) {
    return `mistralai/${model}`;
  }
  if (lower.startsWith("deepseek-")) return `deepseek/${model}`;
  return null;
}

export function configuredConcreteProviderIds(
  keys: ReadonlyArray<{ provider: string; configured: boolean }>,
): ConcreteProviderId[] {
  const configured = new Set(
    keys.filter((k) => k.configured).map((k) => k.provider.toLowerCase()),
  );
  const hasOpenRouterKey = configured.has("openrouter");
  return CONCRETE_PROVIDER_IDS.filter(
    (provider) => configured.has(provider) || hasOpenRouterKey,
  );
}

export function configuredCredentialProviderIds(
  keys: ReadonlyArray<{ provider: string; configured: boolean }>,
): ProviderId[] {
  const configured = new Set(
    keys.filter((k) => k.configured).map((k) => k.provider.toLowerCase()),
  );
  return CREDENTIAL_PROVIDER_IDS.filter((provider) => configured.has(provider));
}
