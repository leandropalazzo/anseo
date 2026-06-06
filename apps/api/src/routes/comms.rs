//! Communications preference center + one-click unsubscribe — Story 43.7.
//!
//! These routes are **public and unauthenticated by design** (AC-3): the
//! preference center and one-click unsubscribe require NO login. Authority is
//! carried by the opaque token in the URL, which resolves (via its hash) to a
//! single recipient in `comms_preference_tokens`. The token is the bearer of
//! authority for exactly one recipient — there is nothing else to leak.
//!
//! Mounted with a single additive `.merge(routes::comms::public_router())` on
//! the unauthenticated `base` router in `apps/api/src/lib.rs`.
//!
//! Routes:
//!   * `GET  /preferences/:token`  — render the preference center (HTML).
//!   * `POST /preferences/:token`  — apply granular toggles (AC-3).
//!   * `GET  /u/:token`            — render an unsubscribe CONFIRMATION page
//!                                    (no side effects — safe for mail-scanner
//!                                    and link-preview prefetches).
//!   * `POST /u/:token`            — perform the unsubscribe. RFC 8058
//!                                    `List-Unsubscribe-Post` one-click target.
//!                                    Idempotent.
//!
//! The mutation is POST-only on purpose: a bare `GET` unsubscribe is fetched by
//! security scanners / link previews and would silently unsubscribe users, and
//! it is not a valid RFC 8058 one-click target (which POSTs). The GET path only
//! renders a confirm form.

use anseo_comms::repo::{CommsRepo, PreferenceUpdate, SuppressionReason};
use anseo_comms::token::hash_token;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Html;
use axum::routing::get;
use axum::{Form, Json, Router};
use serde::{Deserialize, Serialize};

use crate::AppState;

pub fn public_router() -> Router<AppState> {
    Router::new()
        .route(
            "/preferences/:token",
            get(render_preferences).post(update_preferences),
        )
        .route(
            "/u/:token",
            get(unsubscribe_confirm_page).post(one_click_unsubscribe),
        )
}

/// Resolve a raw URL token to its recipient hash, or 404.
async fn resolve(state: &AppState, raw_token: &str) -> Result<String, (StatusCode, Html<String>)> {
    let repo = CommsRepo::new(state.storage.pool());
    let token_hash = hash_token(raw_token);
    match repo.resolve_token(&token_hash).await {
        Ok(Some(recipient_hash)) => Ok(recipient_hash),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Html("<h1>This link is no longer valid.</h1>".to_string()),
        )),
        Err(e) => {
            tracing::error!(event = "comms.token_resolve_failed", error = %e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Html("<h1>Something went wrong.</h1>".to_string()),
            ))
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// GET /preferences/:token  — render the preference center.
// ─────────────────────────────────────────────────────────────────────────────

async fn render_preferences(
    Path(token): Path<String>,
    State(state): State<AppState>,
) -> Result<Html<String>, (StatusCode, Html<String>)> {
    let recipient_hash = resolve(&state, &token).await?;
    let repo = CommsRepo::new(state.storage.pool());

    let sub = match repo.get_subscription(&recipient_hash).await {
        Ok(Some(s)) => s,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Html("<h1>No preferences found for this link.</h1>".to_string()),
            ));
        }
        Err(e) => {
            tracing::error!(event = "comms.subscription_load_failed", error = %e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Html("<h1>Something went wrong.</h1>".to_string()),
            ));
        }
    };

    let checked = |b: bool| if b { "checked" } else { "" };
    let freq_selected = |f: &str| {
        if sub.digest_frequency == f {
            "selected"
        } else {
            ""
        }
    };

    // Server-rendered form. No login, no JS required — submitting POSTs the
    // toggles back to the same token. The unsubscribe-all button is a one-click
    // GET to /u/:token.
    let html = format!(
        r#"<!doctype html>
<html lang="en">
<head><meta charset="utf-8"><title>Email preferences</title></head>
<body>
  <h1>Email preferences</h1>
  <form method="post" action="/preferences/{token}">
    <label>
      <input type="checkbox" name="rank_change_enabled" value="true" {rank_checked}>
      Rank-change notifications
    </label><br>
    <label>
      Digest frequency:
      <select name="digest_frequency">
        <option value="off" {off}>Off</option>
        <option value="daily" {daily}>Daily</option>
        <option value="weekly" {weekly}>Weekly</option>
        <option value="monthly" {monthly}>Monthly</option>
      </select>
    </label><br>
    <label>
      <input type="checkbox" name="all_marketing_off" value="true" {all_off_checked}>
      Turn off ALL marketing email
    </label><br>
    <button type="submit">Save preferences</button>
  </form>
  <hr>
  <p><a href="/u/{token}">Unsubscribe from all marketing (one click)</a></p>
</body>
</html>"#,
        token = token,
        rank_checked = checked(sub.rank_change_enabled),
        all_off_checked = checked(sub.all_marketing_off),
        off = freq_selected("off"),
        daily = freq_selected("daily"),
        weekly = freq_selected("weekly"),
        monthly = freq_selected("monthly"),
    );
    Ok(Html(html))
}

// ─────────────────────────────────────────────────────────────────────────────
// POST /preferences/:token  — apply granular toggles (AC-3).
// ─────────────────────────────────────────────────────────────────────────────

/// Form body. Unchecked HTML checkboxes are omitted entirely, so a missing
/// field means "off" — we translate that into an explicit `false` rather than
/// "leave unchanged", because the form represents the full desired state.
#[derive(Debug, Deserialize)]
struct PreferencesForm {
    #[serde(default)]
    rank_change_enabled: Option<String>,
    digest_frequency: Option<String>,
    #[serde(default)]
    all_marketing_off: Option<String>,
}

async fn update_preferences(
    Path(token): Path<String>,
    State(state): State<AppState>,
    Form(form): Form<PreferencesForm>,
) -> Result<Html<String>, (StatusCode, Html<String>)> {
    let recipient_hash = resolve(&state, &token).await?;
    let repo = CommsRepo::new(state.storage.pool());

    let digest = form
        .digest_frequency
        .filter(|f| matches!(f.as_str(), "off" | "daily" | "weekly" | "monthly"));

    let update = PreferenceUpdate {
        // Checkbox present (value "true") => on; absent => off. Always explicit.
        rank_change_enabled: Some(form.rank_change_enabled.is_some()),
        digest_frequency: digest,
        all_marketing_off: Some(form.all_marketing_off.is_some()),
        marketing_consent: None,
    };

    match repo.update_preferences(&recipient_hash, &update).await {
        Ok(Some(_)) => Ok(Html(
            "<h1>Preferences saved.</h1><p><a href=\"\">Back</a></p>".to_string(),
        )),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Html("<h1>No preferences found for this link.</h1>".to_string()),
        )),
        Err(e) => {
            tracing::error!(event = "comms.preferences_update_failed", error = %e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Html("<h1>Something went wrong.</h1>".to_string()),
            ))
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// GET /u/:token  — one-click unsubscribe, no login (AC-2 / AC-3).
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct UnsubscribeAck {
    status: &'static str,
}

/// `GET /u/:token` — render a confirmation page with NO side effects. Safe for
/// mail-security scanners and link-preview prefetches that auto-fetch GET links.
/// The page contains a form that POSTs back to the same URL to actually
/// unsubscribe (and doubles as the human-visible RFC 8058 fallback).
async fn unsubscribe_confirm_page(
    Path(token): Path<String>,
    State(state): State<AppState>,
) -> Html<String> {
    // Resolve only to tailor the copy; never mutate here. An invalid/expired
    // token still renders the idempotent "already unsubscribed" message.
    let repo = CommsRepo::new(state.storage.pool());
    let known = matches!(repo.resolve_token(&hash_token(&token)).await, Ok(Some(_)));
    if !known {
        return Html("<h1>You are unsubscribed.</h1>".to_string());
    }
    // The form action is the same path (relative), so the POST carries the token.
    Html(format!(
        "<!doctype html><html><body>\
         <h1>Unsubscribe from marketing email?</h1>\
         <p>Click confirm to stop receiving marketing email. Transactional \
         messages (e.g. verification links) are unaffected.</p>\
         <form method=\"post\" action=\"/u/{token}\">\
         <button type=\"submit\">Confirm unsubscribe</button>\
         </form></body></html>"
    ))
}

/// `POST /u/:token` — perform the unsubscribe (RFC 8058 one-click target).
async fn one_click_unsubscribe(
    Path(token): Path<String>,
    State(state): State<AppState>,
) -> Result<Html<String>, (StatusCode, Json<UnsubscribeAck>)> {
    let repo = CommsRepo::new(state.storage.pool());
    let token_hash = hash_token(&token);

    let recipient_hash = match repo.resolve_token(&token_hash).await {
        Ok(Some(rh)) => rh,
        Ok(None) => {
            // Honour idempotency: an already-invalid link still presents as
            // "you're unsubscribed" rather than an error.
            return Ok(Html("<h1>You are unsubscribed.</h1>".to_string()));
        }
        Err(e) => {
            tracing::error!(event = "comms.unsub_token_resolve_failed", error = %e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(UnsubscribeAck { status: "error" }),
            ));
        }
    };

    // One click suffices: add to suppression (scope 'all' marketing) AND flip
    // the master kill-switch so future preference reads reflect it. Idempotent.
    if let Err(e) = repo
        .suppress(&recipient_hash, SuppressionReason::Unsubscribe, "marketing")
        .await
    {
        tracing::error!(event = "comms.unsub_suppress_failed", error = %e);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(UnsubscribeAck { status: "error" }),
        ));
    }
    let _ = repo
        .update_preferences(
            &recipient_hash,
            &PreferenceUpdate {
                all_marketing_off: Some(true),
                ..Default::default()
            },
        )
        .await;

    Ok(Html(
        "<h1>You are unsubscribed.</h1><p>You will no longer receive marketing email.</p>"
            .to_string(),
    ))
}
