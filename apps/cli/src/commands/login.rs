//! `ogeo login <provider>` — capture and persist a provider API key (FR-7, FR-8, FR-11).
//!
//! Behavior:
//! - Accepts the API key on stdin via `rpassword::read_password()` so the
//!   key does NOT echo to the terminal (AC: "without echoing them").
//! - Stores it in the [`opengeo_core::SecretStore`] chain (keyring → age-file).
//! - In a non-TTY context, accepts the key from stdin without prompting —
//!   `echo $KEY | ogeo login openai` works in scripted / CI flows.
//! - Errors NEVER contain the captured secret. We use [`opengeo_core::Secret`]
//!   inside this function and only expose to the backend write path.

use std::io::Read;

use clap::Args;
use opengeo_core::{
    default_chain, OpenGeoError, ProviderName, Secret, SecretStore, SecretStoreError,
};

#[derive(Debug, Args)]
pub struct LoginArgs {
    /// Provider to authenticate.
    pub provider: String,
}

pub fn run(args: LoginArgs) -> Result<(), OpenGeoError> {
    let provider = parse_provider(&args.provider)?;
    let store = default_chain();
    let raw = read_secret_from_user(&provider)?;

    if raw.is_empty() {
        return Err(OpenGeoError::Auth(format!(
            "no key provided for `{provider}`; aborting"
        )));
    }

    let secret = Secret::new(raw);
    store
        .set(&provider.as_wire_str(), secret)
        .map_err(map_store_err)?;

    eprintln!(
        "Stored `{provider}` API key via `{}` backend.",
        store.backend_name()
    );
    Ok(())
}

fn parse_provider(s: &str) -> Result<ProviderName, OpenGeoError> {
    ProviderName::parse(s).ok_or_else(|| {
        OpenGeoError::Auth(format!(
            "unsupported provider `{s}`; expected one of {}",
            ProviderName::all_wire_names().join(", ")
        ))
    })
}

fn read_secret_from_user(provider: &ProviderName) -> Result<String, OpenGeoError> {
    use std::io::IsTerminal as _;
    if std::io::stdin().is_terminal() {
        let prompt = format!("API key for `{provider}` (input hidden): ");
        rpassword::prompt_password(prompt)
            .map_err(|e| OpenGeoError::Auth(format!("failed to read API key: {e}")))
    } else {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| OpenGeoError::Auth(format!("failed to read API key from stdin: {e}")))?;
        // Strip a single trailing newline — common with `echo $KEY | ogeo login`.
        let trimmed = buf.trim_end_matches(['\n', '\r']);
        Ok(trimmed.to_string())
    }
}

fn map_store_err(err: SecretStoreError) -> OpenGeoError {
    OpenGeoError::Auth(err.to_string())
}

/// Look up a stored secret and produce the error shape FR-7/FR-8 specify:
/// "Missing key errors name the relevant env var or login command."
///
/// Used by the `provider run` orchestrator (Epic 2 stories 2.4/2.5) so all
/// missing-key surfaces share one phrasing.
pub fn resolve_provider_secret(provider: ProviderName) -> Result<Secret, OpenGeoError> {
    let env_var = match provider {
        ProviderName::Openai => "OPENAI_API_KEY",
        ProviderName::Anthropic => "ANTHROPIC_API_KEY",
        ProviderName::Gemini => "GEMINI_API_KEY",
        ProviderName::Perplexity => "PERPLEXITY_API_KEY",
        ProviderName::Grok => "GROK_API_KEY",
        ProviderName::Mistral => "MISTRAL_API_KEY",
        ProviderName::Openrouter => "OPENROUTER_API_KEY",
        ProviderName::Plugin(_) => {
            return Err(OpenGeoError::Auth(format!(
                "`{provider}` is a plugin provider; it does not use first-party login"
            )));
        }
    };
    // Env override first — useful in CI without touching keyring.
    if let Ok(v) = std::env::var(env_var) {
        if !v.is_empty() {
            return Ok(Secret::new(v));
        }
    }
    let store = default_chain();
    match store.get(&provider.as_wire_str()) {
        Ok(s) => Ok(s),
        Err(SecretStoreError::NotFound { .. }) => Err(OpenGeoError::Auth(format!(
            "no API key configured for `{provider}`; \
             set `${env_var}` or run `ogeo login {provider}`"
        ))),
        Err(other) => Err(other.into()),
    }
}
