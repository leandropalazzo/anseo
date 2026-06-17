//! Verified Badge Program — Story 43.5 (FR7, UX-DR-9, NFR3, CC-NFR6, BD-1).
//!
//! The badge is the growth flywheel: G2/Gartner-style embeds that drive
//! backlinks and awareness. It is **100% cloud-served** from this service
//! (anseo-internal / benchmark) — no badge logic lives in the OSS client
//! (NFR11). The badge signals **domain-ownership verification only**, never a
//! product-quality endorsement (NFR3).
//!
//! Two variants:
//!   * `brand`  — "✓ Domain-Verified Brand — Anseo"
//!   * `source` — "✓ Domain-Verified Source — Anseo"
//!
//! Endpoints (all under `/v1`):
//!   * `GET /badge/:domain/:variant`            — live SVG badge (size via `?size=`)
//!   * `GET /badge/:domain/:variant/embed`      — JSON embed snippet + license + scope copy
//!
//! Live-state contract (CC-NFR6): the SVG reflects the entity's *current*
//! `claim_status`. A revoked/lapsed claim yields HTTP 410 Gone with a
//! "verification lapsed" SVG body, so a badge embedded on a third-party site
//! stops asserting verified status within one cache TTL (we set a 300s
//! `max-age`, satisfying the 5-minute bound).
//!
//! BD-1: the badge is free-forever — there is **no** payment/entitlement gate
//! anywhere in this code path.
//!
//! Determinism: handlers perform **no network calls** and render SVG from pure
//! string templates, so tests are fully offline and reproducible.

use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::AppState;

/// Default public origin that serves badge images. Overridable via
/// `ANSEO_BADGE_BASE_URL` for staging / self-host. Note: this is
/// `benchmark.anseo.ai`, NOT a `*.opengeo.dev` host.
const DEFAULT_BADGE_BASE_URL: &str = "https://benchmark.anseo.ai";

/// Scope microcopy — MUST appear adjacent to every badge instance (NFR3).
pub const BADGE_SCOPE_MICROCOPY: &str =
    "Verifies domain ownership — not a product quality endorsement.";

/// Cache TTL in seconds for served badge images. Bounds the revocation
/// propagation window to 5 minutes (CC-NFR6).
const BADGE_CACHE_MAX_AGE_SECS: u64 = 300;

/// Resolve the public badge base URL (default + env override).
fn badge_base_url() -> String {
    std::env::var("ANSEO_BADGE_BASE_URL")
        .ok()
        .map(|v| v.trim().trim_end_matches('/').to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| DEFAULT_BADGE_BASE_URL.to_string())
}

pub fn v1_router() -> Router<AppState> {
    Router::new()
        .route("/badge/:domain/:variant", get(serve_badge))
        .route("/badge/:domain/:variant/embed", get(get_embed_snippet))
}

// ─────────────────────────────────────────────────────────────────────────────
// Variant + size models
// ─────────────────────────────────────────────────────────────────────────────

/// The two supported badge variants. Maps a role/lockup, never a third-party
/// logo (NFR2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeVariant {
    Brand,
    Source,
}

impl BadgeVariant {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "brand" => Some(Self::Brand),
            "source" => Some(Self::Source),
            _ => None,
        }
    }

    fn slug(self) -> &'static str {
        match self {
            Self::Brand => "brand",
            Self::Source => "source",
        }
    }

    /// Human label inside the badge lockup. No third-party logo (NFR2); the
    /// only wordmark is "Anseo".
    fn label(self) -> &'static str {
        match self {
            Self::Brand => "Domain-Verified Brand",
            Self::Source => "Domain-Verified Source",
        }
    }
}

/// Badge render sizes. Scale factor applied to the base 1× geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeSize {
    /// 1× — standard embed.
    Standard,
    /// 2× — retina / hi-dpi.
    Retina,
}

impl BadgeSize {
    fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "1x" | "1" | "standard" | "" => Some(Self::Standard),
            "2x" | "2" | "retina" => Some(Self::Retina),
            _ => None,
        }
    }

    fn scale(self) -> u32 {
        match self {
            Self::Standard => 1,
            Self::Retina => 2,
        }
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct BadgeQuery {
    #[serde(default)]
    pub size: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// SVG rendering (pure, deterministic, no network)
// ─────────────────────────────────────────────────────────────────────────────

/// XML-escape a string for safe inclusion in SVG text nodes / attributes.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Render the *verified* badge SVG for a variant at a given size. Pure.
pub fn render_verified_svg(domain: &str, variant: BadgeVariant, size: BadgeSize) -> String {
    let scale = size.scale();
    let base_w = 240u32;
    let base_h = 56u32;
    let w = base_w * scale;
    let h = base_h * scale;
    let label = variant.label();
    let safe_domain = xml_escape(domain);
    let safe_label = xml_escape(label);
    // Accessibility: <title>/<desc> carry the scope framing so screen readers
    // do not infer a quality endorsement (NFR3).
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {base_w} {base_h}" role="img" aria-label="{safe_label} — {safe_domain} — verified by Anseo">
  <title>{safe_label} — Anseo</title>
  <desc>{scope}</desc>
  <rect width="{base_w}" height="{base_h}" rx="8" fill="#0B1F3A"/>
  <g fill="#FFFFFF" font-family="-apple-system, Segoe UI, Roboto, sans-serif">
    <circle cx="28" cy="28" r="12" fill="#1FB873"/>
    <path d="M22 28 l4 4 l8 -9" stroke="#FFFFFF" stroke-width="2.5" fill="none" stroke-linecap="round" stroke-linejoin="round"/>
    <text x="52" y="24" font-size="13" font-weight="600">{safe_label}</text>
    <text x="52" y="42" font-size="11" fill="#9FB3C8">Anseo</text>
  </g>
</svg>"##,
        scope = xml_escape(BADGE_SCOPE_MICROCOPY)
    )
}

/// Render the *lapsed/revoked* badge SVG. Visually muted, no ✓ assertion.
pub fn render_lapsed_svg(domain: &str, size: BadgeSize) -> String {
    let scale = size.scale();
    let base_w = 240u32;
    let base_h = 56u32;
    let w = base_w * scale;
    let h = base_h * scale;
    let safe_domain = xml_escape(domain);
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {base_w} {base_h}" role="img" aria-label="Verification lapsed — {safe_domain}">
  <title>Verification lapsed — Anseo</title>
  <desc>This domain is no longer Domain-Verified.</desc>
  <rect width="{base_w}" height="{base_h}" rx="8" fill="#2A2A2A"/>
  <g fill="#9A9A9A" font-family="-apple-system, Segoe UI, Roboto, sans-serif">
    <circle cx="28" cy="28" r="12" fill="#555555"/>
    <text x="52" y="24" font-size="13" font-weight="600">Verification lapsed</text>
    <text x="52" y="42" font-size="11" fill="#777777">Anseo</text>
  </g>
</svg>"##
    )
}

/// `true` when the entity's `claim_status` currently licenses a *verified*
/// badge. Only an actively-verified claim qualifies; anything else (unclaimed,
/// pending, revoked, conflict, lapsed) yields the lapsed image + 410.
fn is_badge_active(claim_status: &str) -> bool {
    matches!(claim_status, "verified" | "claimed")
}

/// `true` when the entity's `role` authorizes the requested badge variant.
/// A verified source-only domain must NOT be able to serve a "brand" badge
/// (and vice-versa). Roles are `'brand' | 'source' | 'both'`; `both` matches
/// either variant. Combined with [`is_badge_active`], this gates the verified
/// SVG on BOTH an active claim AND a role that matches what was requested.
fn role_matches_variant(role: &str, variant: BadgeVariant) -> bool {
    match role {
        "both" => true,
        "brand" => variant == BadgeVariant::Brand,
        "source" => variant == BadgeVariant::Source,
        _ => false,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// GET /v1/badge/:domain/:variant
// ─────────────────────────────────────────────────────────────────────────────

fn svg_response(status: StatusCode, body: String) -> Response {
    (
        status,
        [
            (
                header::CONTENT_TYPE,
                "image/svg+xml; charset=utf-8".to_string(),
            ),
            (
                header::CACHE_CONTROL,
                format!("public, max-age={BADGE_CACHE_MAX_AGE_SECS}"),
            ),
        ],
        body,
    )
        .into_response()
}

async fn serve_badge(
    Path((raw_domain, raw_variant)): Path<(String, String)>,
    Query(q): Query<BadgeQuery>,
    State(state): State<AppState>,
) -> Response {
    let domain = anseo_storage::repositories::entities::EntityRepo::normalize_domain(&raw_domain);

    let variant = match BadgeVariant::parse(&raw_variant) {
        Some(v) => v,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "invalid_variant",
                    "allowed": ["brand", "source"],
                })),
            )
                .into_response();
        }
    };

    let size = match BadgeSize::parse(q.size.as_deref().unwrap_or("1x")) {
        Some(s) => s,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "invalid_size",
                    "allowed": ["1x", "2x"],
                })),
            )
                .into_response();
        }
    };

    // Live verification state (CC-NFR6). No network call — single DB read.
    let entity = match state.storage.entities().get(&domain).await {
        Ok(e) => e,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "storage_error", "message": e.to_string() })),
            )
                .into_response();
        }
    };

    // A verified SVG requires BOTH an active claim AND a role that authorizes
    // the requested variant — otherwise a source-only domain could serve a
    // "brand" badge (and vice-versa). Either failure → lapsed image + 410.
    let active = entity
        .as_ref()
        .map(|e| is_badge_active(&e.claim_status) && role_matches_variant(&e.role, variant))
        .unwrap_or(false);

    // Story 47.1 — server-side `badge_embed_view` analytics event. The SVG is
    // embedded via `<img>` on third-party sites which can't fire a client-side
    // beacon, so the serve itself is the signal. Best-effort: a failure here
    // must never break badge delivery, and no raw domain / IP is stored (the
    // raw domain goes only in the ephemeral session_id-less event row's
    // `properties` is omitted; see A3 — `badge_embed_view` has no properties).
    let _ = state
        .storage
        .site_events()
        .insert(
            "badge_embed_view",
            uuid::Uuid::new_v4(),
            None,
            None,
            &serde_json::json!({}),
        )
        .await;

    if active {
        svg_response(StatusCode::OK, render_verified_svg(&domain, variant, size))
    } else {
        // Revoked / lapsed / unverified → 410 Gone with the lapsed image so
        // third-party embeds stop asserting verified status (CC-NFR6).
        svg_response(StatusCode::GONE, render_lapsed_svg(&domain, size))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// GET /v1/badge/:domain/:variant/embed
// ─────────────────────────────────────────────────────────────────────────────

/// Badge license terms (AC-4). BD-1: free — no payment.
#[derive(Debug, Serialize)]
pub struct BadgeLicense {
    pub no_alteration: &'static str,
    pub mandatory_backlink: &'static str,
    pub validity: &'static str,
    pub revocable: &'static str,
    /// BD-1 — the badge costs nothing.
    pub price: &'static str,
}

impl Default for BadgeLicense {
    fn default() -> Self {
        Self {
            no_alteration: "The badge image must not be altered.",
            mandatory_backlink:
                "The badge must link back to the Anseo verification page for this domain.",
            validity: "Valid for 12 months from the verification date.",
            revocable: "Revocable by Anseo at any time.",
            price: "Free — no payment required.",
        }
    }
}

#[derive(Debug, Serialize)]
pub struct EmbedSnippetResponse {
    pub domain: String,
    pub variant: &'static str,
    /// Ready-to-paste HTML: `<a href><img …></a>`.
    pub embed_html: String,
    /// Absolute badge image URL.
    pub badge_url: String,
    /// Absolute verification (profile) page the badge links to.
    pub verification_url: String,
    /// Img alt text (also exposed standalone for the copy UI).
    pub alt_text: String,
    /// aria-label on the anchor.
    pub aria_label: String,
    /// MUST be shown adjacent to the badge (NFR3).
    pub scope_microcopy: &'static str,
    pub license: BadgeLicense,
    /// `true` only when the domain currently licenses a verified badge.
    pub badge_active: bool,
}

async fn get_embed_snippet(
    Path((raw_domain, raw_variant)): Path<(String, String)>,
    State(state): State<AppState>,
) -> Result<Json<EmbedSnippetResponse>, (StatusCode, Json<serde_json::Value>)> {
    let domain = anseo_storage::repositories::entities::EntityRepo::normalize_domain(&raw_domain);

    let variant = BadgeVariant::parse(&raw_variant).ok_or((
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({
            "error": "invalid_variant",
            "allowed": ["brand", "source"],
        })),
    ))?;

    let entity = state.storage.entities().get(&domain).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "storage_error", "message": e.to_string() })),
        )
    })?;

    let badge_active = entity
        .as_ref()
        .map(|e| is_badge_active(&e.claim_status) && role_matches_variant(&e.role, variant))
        .unwrap_or(false);

    let base = badge_base_url();
    let badge_url = format!("{base}/v1/badge/{domain}/{}", variant.slug());
    // Backlink target: `{base}/brand/{domain}` is a page on the PUBLIC benchmark
    // web app (anseo-web `app/brand/[domain]/page.jsx`, Story 43.4), NOT an
    // API route in this service — `base` defaults to https://benchmark.anseo.ai.
    // The mandatory badge backlink must point at that public profile page.
    let verification_url = format!("{base}/brand/{domain}");

    // Neutral, state-agnostic accessibility text. The pasted snippet is static
    // and outlives any verification state, so it must NOT permanently assert
    // "verified" — after a revocation the live SVG flips to the lapsed body
    // within one cache TTL, but this alt/aria text stays in the consumer's HTML
    // forever. Keep it descriptive ("badge for"), not an assertion ("verified").
    let alt_text = format!("{} badge for {} — Anseo", variant.label(), domain);
    let aria_label = format!("Anseo {} badge for {}", variant.label(), domain);

    // Snippet attributes are XML/HTML-escaped to keep the paste safe.
    let embed_html = build_embed_html(&badge_url, &verification_url, &alt_text, &aria_label);

    Ok(Json(EmbedSnippetResponse {
        domain,
        variant: variant.slug(),
        embed_html,
        badge_url,
        verification_url,
        alt_text,
        aria_label,
        scope_microcopy: BADGE_SCOPE_MICROCOPY,
        license: BadgeLicense::default(),
        badge_active,
    }))
}

/// Build the `<a href><img></a>` embed snippet. Pure — testable offline.
pub fn build_embed_html(
    badge_url: &str,
    verification_url: &str,
    alt_text: &str,
    aria_label: &str,
) -> String {
    format!(
        "<a href=\"{href}\" aria-label=\"{aria}\" target=\"_blank\" rel=\"noopener\">\
<img src=\"{src}\" alt=\"{alt}\" width=\"240\" height=\"56\" /></a>",
        href = xml_escape(verification_url),
        aria = xml_escape(aria_label),
        src = xml_escape(badge_url),
        alt = xml_escape(alt_text),
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests (pure — no DB, no network)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variant_parsing_is_case_insensitive_and_rejects_unknown() {
        assert_eq!(BadgeVariant::parse("brand"), Some(BadgeVariant::Brand));
        assert_eq!(BadgeVariant::parse("BRAND"), Some(BadgeVariant::Brand));
        assert_eq!(BadgeVariant::parse("source"), Some(BadgeVariant::Source));
        assert_eq!(BadgeVariant::parse("Source"), Some(BadgeVariant::Source));
        assert_eq!(BadgeVariant::parse("partner"), None);
        assert_eq!(BadgeVariant::parse(""), None);
    }

    #[test]
    fn size_parsing_accepts_aliases_and_scales() {
        assert_eq!(BadgeSize::parse("1x"), Some(BadgeSize::Standard));
        assert_eq!(BadgeSize::parse(""), Some(BadgeSize::Standard));
        assert_eq!(BadgeSize::parse("2x"), Some(BadgeSize::Retina));
        assert_eq!(BadgeSize::parse("2"), Some(BadgeSize::Retina));
        assert_eq!(BadgeSize::parse("4x"), None);
        assert_eq!(BadgeSize::Standard.scale(), 1);
        assert_eq!(BadgeSize::Retina.scale(), 2);
    }

    #[test]
    fn active_status_only_for_verified_or_claimed() {
        assert!(is_badge_active("verified"));
        assert!(is_badge_active("claimed"));
        assert!(!is_badge_active("revoked"));
        assert!(!is_badge_active("pending"));
        assert!(!is_badge_active("unclaimed"));
        assert!(!is_badge_active("conflict"));
        assert!(!is_badge_active("lapsed"));
    }

    #[test]
    fn role_must_match_requested_variant() {
        // The verified badge is gated on BOTH an active claim AND a role that
        // authorizes the requested variant. This mirrors the `serve_badge`
        // active-check: `is_badge_active(status) && role_matches_variant(role, v)`.

        // verified brand + brand request → active (200).
        assert!(
            is_badge_active("verified") && role_matches_variant("brand", BadgeVariant::Brand),
            "verified brand domain should serve a brand badge"
        );
        // verified source-only + brand request → NOT active (410): the bug fix.
        assert!(
            !(is_badge_active("verified") && role_matches_variant("source", BadgeVariant::Brand)),
            "verified source-only domain must NOT serve a brand badge"
        );
        // verified source-only + source request → active (200).
        assert!(
            is_badge_active("verified") && role_matches_variant("source", BadgeVariant::Source)
        );
        // verified both → active for either variant (200).
        assert!(is_badge_active("verified") && role_matches_variant("both", BadgeVariant::Brand));
        assert!(is_badge_active("verified") && role_matches_variant("both", BadgeVariant::Source));

        // Role gate is independent of claim gate: even a perfect role match is
        // inactive when the claim is not verified/claimed.
        assert!(!(is_badge_active("revoked") && role_matches_variant("both", BadgeVariant::Brand)));

        // Unknown role never matches.
        assert!(!role_matches_variant("partner", BadgeVariant::Brand));
        assert!(!role_matches_variant("partner", BadgeVariant::Source));
    }

    #[test]
    fn verified_svg_carries_scope_framing_and_no_quality_claim() {
        let svg = render_verified_svg("example.com", BadgeVariant::Brand, BadgeSize::Standard);
        // Scope microcopy embedded for a11y (NFR3).
        assert!(svg.contains("not a product quality endorsement"));
        // Variant label present; no third-party logo wordmark (NFR2) — only Anseo.
        assert!(svg.contains("Domain-Verified Brand"));
        assert!(svg.contains("Anseo"));
        // Never asserts ratings/quality language.
        assert!(!svg.to_lowercase().contains("best"));
        assert!(!svg.to_lowercase().contains("rating"));
    }

    #[test]
    fn svg_size_scales_pixel_dimensions_but_keeps_viewbox() {
        let one = render_verified_svg("example.com", BadgeVariant::Source, BadgeSize::Standard);
        let two = render_verified_svg("example.com", BadgeVariant::Source, BadgeSize::Retina);
        assert!(one.contains(r#"width="240" height="56""#));
        assert!(two.contains(r#"width="480" height="112""#));
        // viewBox constant across sizes (vector scales cleanly).
        assert!(one.contains(r#"viewBox="0 0 240 56""#));
        assert!(two.contains(r#"viewBox="0 0 240 56""#));
    }

    #[test]
    fn lapsed_svg_makes_no_verified_assertion() {
        let svg = render_lapsed_svg("example.com", BadgeSize::Standard);
        assert!(svg.contains("Verification lapsed"));
        assert!(svg.contains("no longer Domain-Verified"));
        assert!(!svg.contains("Domain-Verified Brand"));
        assert!(!svg.contains("Domain-Verified Source"));
    }

    #[test]
    fn embed_html_has_anchor_img_alt_and_aria() {
        let html = build_embed_html(
            "https://benchmark.anseo.ai/v1/badge/example.com/brand",
            "https://benchmark.anseo.ai/brand/example.com",
            "Domain-Verified Brand — example.com — verified by Anseo",
            "Domain-Verified Brand verification on Anseo for example.com",
        );
        assert!(html.contains("<a href=\"https://benchmark.anseo.ai/brand/example.com\""));
        assert!(html.contains("aria-label=\""));
        assert!(html.contains("<img src=\"https://benchmark.anseo.ai/v1/badge/example.com/brand\""));
        assert!(html.contains("alt=\"Domain-Verified Brand — example.com — verified by Anseo\""));
        assert!(html.contains("rel=\"noopener\""));
    }

    #[test]
    fn embed_html_escapes_injection_attempts() {
        let html = build_embed_html(
            "https://x/v1/badge/a\"><script>/brand",
            "https://x/brand/a",
            "alt \"quote\"",
            "aria",
        );
        assert!(!html.contains("<script>"));
        assert!(html.contains("&quot;"));
    }

    #[test]
    fn license_terms_state_all_four_clauses_and_free() {
        let lic = BadgeLicense::default();
        assert!(lic.no_alteration.to_lowercase().contains("not be altered"));
        assert!(lic.mandatory_backlink.to_lowercase().contains("link back"));
        assert!(lic.validity.contains("12 months"));
        assert!(lic.revocable.to_lowercase().contains("revocable"));
        // BD-1: free.
        assert!(lic.price.to_lowercase().contains("free"));
    }

    #[test]
    fn scope_microcopy_is_exact_nfr3_string() {
        assert_eq!(
            BADGE_SCOPE_MICROCOPY,
            "Verifies domain ownership — not a product quality endorsement."
        );
    }

    // CC-NFR6: revocation must propagate within 5 minutes. This is a property
    // of a compile-time constant, so enforce it as a compile-time assertion
    // (a runtime `assert!` on a const trips clippy::assertions_on_constants).
    const _: () = assert!(BADGE_CACHE_MAX_AGE_SECS <= 300);

    #[test]
    fn badge_base_url_default_and_override() {
        // Combined into ONE test fn — env is process-global and races across
        // parallel test fns otherwise.
        std::env::remove_var("ANSEO_BADGE_BASE_URL");
        assert_eq!(badge_base_url(), "https://benchmark.anseo.ai");
        assert!(!badge_base_url().contains("opengeo.dev"));

        std::env::set_var("ANSEO_BADGE_BASE_URL", "https://staging.example.test/");
        // Trailing slash trimmed.
        assert_eq!(badge_base_url(), "https://staging.example.test");

        // Empty override falls back to default.
        std::env::set_var("ANSEO_BADGE_BASE_URL", "   ");
        assert_eq!(badge_base_url(), "https://benchmark.anseo.ai");

        std::env::remove_var("ANSEO_BADGE_BASE_URL");
    }
}
