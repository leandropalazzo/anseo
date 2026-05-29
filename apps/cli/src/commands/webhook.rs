//! `ogeo webhook …` — Phase 2 Story 12.4 webhook management.
//!
//! Subcommands:
//! - `add --name <slug> --target-url <url> --event-kinds <list>` —
//!   declare a new webhook, generate a fresh 32-byte secret, print the
//!   plaintext ONCE, store the row.
//! - `list` — print active and disabled webhooks (target URL + display
//!   summary; secrets never appear).
//! - `rotate-secret --name <slug>` — generate a new secret, store, print
//!   the plaintext ONCE.
//! - `reenable --name <slug>` — flip `disabled` back to false. The
//!   auto-disable path runs in the dispatcher; this is the only way back.

use clap::Args;
use opengeo_core::{Config, OpenGeoError};
use opengeo_storage::Storage;

#[derive(Debug, Args)]
pub struct AddArgs {
    /// Slug-safe name for the webhook. Used as the display label and
    /// revoke target.
    #[arg(long)]
    pub name: String,
    /// HTTPS URL the dispatcher POSTs deliveries to.
    #[arg(long)]
    pub target_url: String,
    /// Comma-separated event kinds this webhook subscribes to
    /// (`prompt_run.completed`, `visibility.regression`,
    /// `schedule.missed`, `visibility.anomaly`, `citation.anomaly`).
    #[arg(long)]
    pub event_kinds: String,
    #[arg(long, default_value = "opengeo.yaml")]
    pub config: std::path::PathBuf,
}

#[derive(Debug, Args)]
pub struct ListArgs {
    #[arg(long, default_value = "opengeo.yaml")]
    pub config: std::path::PathBuf,
    /// Hide disabled rows (default shows them for audit visibility).
    #[arg(long)]
    pub active_only: bool,
}

#[derive(Debug, Args)]
pub struct RotateSecretArgs {
    #[arg(long)]
    pub name: String,
    #[arg(long, default_value = "opengeo.yaml")]
    pub config: std::path::PathBuf,
}

#[derive(Debug, Args)]
pub struct ReenableArgs {
    #[arg(long)]
    pub name: String,
    #[arg(long, default_value = "opengeo.yaml")]
    pub config: std::path::PathBuf,
}

pub async fn run_add(args: AddArgs) -> Result<(), OpenGeoError> {
    let project_id = project_id_from_config(&args.config)?;
    let storage = connect_storage().await?;

    let event_kinds = parse_event_kinds(&args.event_kinds)?;
    validate_target_url(&args.target_url)?;

    let secret = generate_secret();
    let secret_b64 = base64_encode(&secret);

    let event_kinds_json = serde_json::Value::Array(
        event_kinds
            .iter()
            .map(|k| serde_json::Value::String(k.clone()))
            .collect(),
    );

    storage
        .webhooks()
        .insert(
            project_id,
            &args.name,
            &args.target_url,
            &secret_b64,
            &event_kinds_json,
        )
        .await
        .map_err(|e| OpenGeoError::Internal(anyhow::anyhow!(e)))?;

    println!("Created webhook `{}`.", args.name);
    println!();
    println!("    Target URL : {}", args.target_url);
    println!("    Event kinds: {}", event_kinds.join(", "));
    println!("    Secret     : {secret_b64}");
    println!();
    println!("The secret will not be shown again. Share it with the consumer that");
    println!("verifies the X-OpenGEO-Signature header (architecture §5.2). Rotate");
    println!("with `ogeo webhook rotate-secret --name {}`.", args.name);
    Ok(())
}

pub async fn run_list(args: ListArgs) -> Result<(), OpenGeoError> {
    let project_id = project_id_from_config(&args.config)?;
    let storage = connect_storage().await?;
    let rows = storage
        .webhooks()
        .list_for_project(project_id)
        .await
        .map_err(|e| OpenGeoError::Internal(anyhow::anyhow!(e)))?;

    if rows.is_empty() {
        println!("(no webhooks for this project)");
        return Ok(());
    }

    println!(
        "{:<20} {:<40} {:<10} EVENT KINDS",
        "NAME", "TARGET URL", "STATUS"
    );
    for row in rows {
        if args.active_only && row.disabled {
            continue;
        }
        let status = if row.disabled { "disabled" } else { "active" };
        let kinds = match &row.event_kinds {
            serde_json::Value::Array(a) => a
                .iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join(","),
            _ => "?".to_string(),
        };
        println!(
            "{:<20} {:<40} {:<10} {}",
            row.name, row.target_url, status, kinds
        );
        if row.disabled {
            if let Some(reason) = row.disabled_reason {
                println!("    └─ disabled: {reason}");
            }
        }
    }
    Ok(())
}

pub async fn run_rotate_secret(args: RotateSecretArgs) -> Result<(), OpenGeoError> {
    let project_id = project_id_from_config(&args.config)?;
    let storage = connect_storage().await?;

    let secret = generate_secret();
    let secret_b64 = base64_encode(&secret);

    let updated = storage
        .webhooks()
        .rotate_secret(project_id, &args.name, &secret_b64)
        .await
        .map_err(|e| OpenGeoError::Internal(anyhow::anyhow!(e)))?;

    if !updated {
        return Err(OpenGeoError::Config(format!(
            "no webhook named `{}` exists for this project",
            args.name
        )));
    }

    println!("Rotated secret for webhook `{}`.", args.name);
    println!();
    println!("    Secret: {secret_b64}");
    println!();
    println!("Update the consumer to verify against the new secret. The old secret");
    println!("is no longer accepted; deliveries signed before now with the previous");
    println!("secret will fail at the consumer's verify step.");
    Ok(())
}

pub async fn run_reenable(args: ReenableArgs) -> Result<(), OpenGeoError> {
    let project_id = project_id_from_config(&args.config)?;
    let storage = connect_storage().await?;
    let flipped = storage
        .webhooks()
        .reenable(project_id, &args.name)
        .await
        .map_err(|e| OpenGeoError::Internal(anyhow::anyhow!(e)))?;

    if flipped {
        println!("Re-enabled webhook `{}`.", args.name);
    } else {
        println!(
            "Webhook `{}` is not disabled (or does not exist); no change made.",
            args.name
        );
    }
    Ok(())
}

fn project_id_from_config(path: &std::path::Path) -> Result<opengeo_core::ProjectId, OpenGeoError> {
    let yaml = std::fs::read_to_string(path)
        .map_err(|e| OpenGeoError::Config(format!("could not read {}: {e}", path.display())))?;
    let cfg = Config::from_yaml_str(&yaml)
        .map_err(|e| OpenGeoError::Config(format!("could not parse {}: {e}", path.display())))?;
    Ok(cfg.project_id())
}

async fn connect_storage() -> Result<Storage, OpenGeoError> {
    let url = std::env::var("DATABASE_URL")
        .map_err(|_| OpenGeoError::Config("DATABASE_URL is required for `ogeo webhook`".into()))?;
    let storage = Storage::connect(&url)
        .await
        .map_err(|e| OpenGeoError::Internal(anyhow::anyhow!(e)))?;
    storage
        .migrate()
        .await
        .map_err(|e| OpenGeoError::Internal(anyhow::anyhow!(e)))?;
    Ok(storage)
}

/// Five wire-stable event kinds per architecture §5.3. Parser rejects
/// anything else with a clear error pointing to the supported set.
const SUPPORTED_EVENT_KINDS: &[&str] = &[
    "prompt_run.completed",
    "visibility.regression",
    "schedule.missed",
    "visibility.anomaly",
    "citation.anomaly",
];

fn parse_event_kinds(raw: &str) -> Result<Vec<String>, OpenGeoError> {
    let kinds: Vec<String> = raw
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if kinds.is_empty() {
        return Err(OpenGeoError::Config(
            "--event-kinds must include at least one wire-stable event kind".into(),
        ));
    }
    for k in &kinds {
        if !SUPPORTED_EVENT_KINDS.contains(&k.as_str()) {
            return Err(OpenGeoError::Config(format!(
                "unknown event kind `{k}` (supported: {})",
                SUPPORTED_EVENT_KINDS.join(", ")
            )));
        }
    }
    Ok(kinds)
}

fn validate_target_url(url: &str) -> Result<(), OpenGeoError> {
    // We don't want to pull a full URL crate just to reject obvious typos.
    // Phase 2 webhook targets MUST be HTTPS in production; HTTP allowed
    // for local dev fixtures (wiremock at 127.0.0.1).
    if url.starts_with("https://") {
        return Ok(());
    }
    if url.starts_with("http://127.0.0.1") || url.starts_with("http://localhost") {
        return Ok(());
    }
    Err(OpenGeoError::Config(format!(
        "--target-url `{url}` must use https://, or http://127.0.0.1 / http://localhost for local fixtures"
    )))
}

/// Generate a 32-byte random secret. Uses `/dev/urandom` on Unix; the
/// project is Unix-only for Phase 2.
fn generate_secret() -> [u8; 32] {
    let mut buf = [0u8; 32];
    #[cfg(unix)]
    {
        use std::io::Read;
        let mut f = std::fs::File::open("/dev/urandom")
            .expect("/dev/urandom unavailable for webhook secret generation");
        f.read_exact(&mut buf).expect("read /dev/urandom failed");
    }
    #[cfg(not(unix))]
    compile_error!("webhook secret generation needs /dev/urandom on non-Unix targets");
    buf
}

/// Minimal standard base64 (URL-unsafe alphabet). Avoids pulling a base64
/// crate for the one place we need it.
fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let chunks = bytes.chunks(3);
    for chunk in chunks {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        out.push(ALPHABET[(b0 >> 2) as usize] as char);
        out.push(ALPHABET[(((b0 & 0b11) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[(((b1 & 0b1111) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[(b2 & 0b111111) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_event_kinds_accepts_supported() {
        let kinds = parse_event_kinds("prompt_run.completed,visibility.anomaly").unwrap();
        assert_eq!(kinds.len(), 2);
        assert!(kinds.iter().any(|k| k == "prompt_run.completed"));
    }

    #[test]
    fn parse_event_kinds_trims_whitespace() {
        let kinds = parse_event_kinds("  prompt_run.completed , visibility.regression ").unwrap();
        assert_eq!(kinds.len(), 2);
    }

    #[test]
    fn parse_event_kinds_rejects_unknown() {
        let err = parse_event_kinds("bogus.event").unwrap_err();
        match err {
            OpenGeoError::Config(msg) => assert!(msg.contains("unknown event kind")),
            other => panic!("expected Config error, got {other:?}"),
        }
    }

    #[test]
    fn parse_event_kinds_rejects_empty() {
        let err = parse_event_kinds("").unwrap_err();
        match err {
            OpenGeoError::Config(msg) => assert!(msg.contains("at least one")),
            other => panic!("expected Config error, got {other:?}"),
        }
    }

    #[test]
    fn validate_target_url_accepts_https() {
        assert!(validate_target_url("https://example.com/webhook").is_ok());
    }

    #[test]
    fn validate_target_url_accepts_localhost_http_for_fixtures() {
        assert!(validate_target_url("http://127.0.0.1:8080/hook").is_ok());
        assert!(validate_target_url("http://localhost/hook").is_ok());
    }

    #[test]
    fn validate_target_url_rejects_plaintext_external() {
        let err = validate_target_url("http://example.com/webhook").unwrap_err();
        match err {
            OpenGeoError::Config(msg) => assert!(msg.contains("https://")),
            other => panic!("expected Config error, got {other:?}"),
        }
    }

    #[test]
    fn base64_encode_produces_expected_length() {
        // 32 bytes → 44 chars (32 / 3 = 10 remainder 2, ceil → 11 groups, *4 = 44).
        let bytes = [0u8; 32];
        let out = base64_encode(&bytes);
        assert_eq!(out.len(), 44);
        // All-zero input → AA...= shape.
        assert!(out.ends_with('='));
    }

    #[test]
    fn base64_encode_round_trip_against_known_vectors() {
        // From RFC 4648 §10 — keep behaviour pinned across alphabet changes.
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn supported_event_kinds_matches_arch_5_3() {
        assert_eq!(SUPPORTED_EVENT_KINDS.len(), 5);
        for kind in [
            "prompt_run.completed",
            "visibility.regression",
            "schedule.missed",
            "visibility.anomaly",
            "citation.anomaly",
        ] {
            assert!(
                SUPPORTED_EVENT_KINDS.contains(&kind),
                "kind {kind} should be supported per arch §5.3"
            );
        }
    }
}
