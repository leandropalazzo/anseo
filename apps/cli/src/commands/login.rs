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
use std::path::PathBuf;

use clap::Args;
use opengeo_core::{
    default_chain, set_provider_secret, Config, OpenGeoError, ProviderName, Secret, SecretStore,
    SecretStoreError,
};

#[derive(Debug, Args)]
pub struct LoginArgs {
    /// Provider to authenticate.
    pub provider: String,

    /// Path to `opengeo.yaml`, used to resolve the current project so the key
    /// is stored under that project's namespace. Defaults to `./opengeo.yaml`.
    #[arg(long)]
    pub config: Option<PathBuf>,

    /// Explicit project id to store the key under, overriding the project
    /// derived from `opengeo.yaml`. Useful for scripted multi-project setups.
    #[arg(long)]
    pub project: Option<String>,
}

pub fn run(args: LoginArgs) -> Result<(), OpenGeoError> {
    let provider = parse_provider(&args.provider)?;
    let project_id = resolve_project_id(&args)?;
    let store = default_chain();
    let raw = read_secret_from_user(&provider)?;

    if raw.is_empty() {
        return Err(OpenGeoError::Auth(format!(
            "no key provided for `{provider}`; aborting"
        )));
    }

    let secret = Secret::new(raw);
    match &project_id {
        // Story 36.7: store under the project-scoped namespace so each project
        // carries its own provider credentials. The provider-registry read path
        // prefers this key and falls back to the legacy global key.
        Some(pid) => set_provider_secret(&store, pid, &provider.as_wire_str(), secret)
            .map_err(map_store_err)?,
        // No project could be resolved (no `opengeo.yaml`, no `--project`).
        // Fall back to the legacy global namespace so a project-less setup keeps
        // working exactly as before per-project keying existed.
        None => store
            .set(&provider.as_wire_str(), secret)
            .map_err(map_store_err)?,
    }

    match &project_id {
        Some(pid) => eprintln!(
            "Stored `{provider}` API key for project `{pid}` via `{}` backend.",
            store.backend_name()
        ),
        None => eprintln!(
            "Stored `{provider}` API key (global, no project resolved) via `{}` backend.",
            store.backend_name()
        ),
    }
    Ok(())
}

/// Resolve the project id the key should be stored under.
///
/// Precedence:
/// 1. An explicit `--project <id>` flag.
/// 2. The `ProjectId` derived from `opengeo.yaml` (default `./opengeo.yaml`,
///    or the path given by `--config`).
///
/// Returns `Ok(None)` when no config is present and no `--project` was given —
/// the caller then stores the key under the legacy global namespace so a
/// project-less setup keeps working. An explicitly-pointed `--config` that
/// fails to load IS surfaced as an error (the user asked for that file).
fn resolve_project_id(args: &LoginArgs) -> Result<Option<String>, OpenGeoError> {
    if let Some(explicit) = &args.project {
        return Ok(Some(explicit.clone()));
    }
    match &args.config {
        Some(path) => {
            let config = Config::from_path(path).map_err(|e| {
                OpenGeoError::Auth(format!(
                    "could not resolve the current project from `{}`: {e}. \
                     Pass a valid `--config <path>` or `--project <id>`.",
                    path.display()
                ))
            })?;
            Ok(Some(config.project_id().to_string()))
        }
        // Default path: best-effort. Missing/invalid `opengeo.yaml` falls back
        // to global keying rather than failing the login.
        None => match Config::from_path(PathBuf::from("opengeo.yaml")) {
            Ok(config) => Ok(Some(config.project_id().to_string())),
            Err(_) => Ok(None),
        },
    }
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
