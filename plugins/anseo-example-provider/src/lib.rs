//! `anseo-example-provider` — the canonical first-party Provider plugin and the
//! SDK author template (Story 41.5).
//!
//! This is the minimal shape a Provider plugin takes: it receives a prompt
//! request, performs its work (here: a call to a public echo API, scoped by the
//! `network` capability declared in `manifest.yaml`), and returns a response.
//!
//! Build target: `wasm32-wasi` → `entrypoint.wasm` (the manifest `entry_point`).
//! The 41.4 CI pipeline compiles this, computes `SHA-256(manifest.yaml ||
//! entrypoint.wasm)`, signs it with the namespace author key, and publishes the
//! bundle + `signature.bin` + `claim.toml` to `github.com/anseo/plugin-registry`.
//!
//! The host invokes the plugin only through the existing provider surface via
//! the `plugin:anseo/anseo-example-provider:provider` namespace — no new MCP
//! tool, Web route, or CLI verb is introduced (see
//! `docs/plugin-surface-boundary.md`).

/// A prompt request handed to the provider by the host.
///
/// Mirrors the host-side `ProviderRequest` (`crates/providers`) — the fields a
/// plugin author needs to produce a response.
pub struct ProviderRequest {
    /// The prompt text to send to the upstream model / API.
    pub prompt: String,
    /// The model id the run selected (validated by [`validate_model`]).
    pub model: String,
}

/// The response a provider returns. The host wraps this into a
/// `PromptRunRecord` indistinguishable from a first-party provider's.
pub struct ProviderResponse {
    /// Human-readable message text.
    pub message_text: String,
    /// The raw upstream payload, surfaced verbatim for auditability.
    pub raw_response: String,
}

/// The model ids this example provider accepts. A real provider would advertise
/// the upstream model catalog.
pub const ACCEPTED_MODELS: &[&str] = &["echo-1"];

/// The single network host this plugin is permitted to reach. MUST match the
/// `network` allowlist in `manifest.yaml`; the host mediates every outbound
/// fetch against the declared capability.
pub const ECHO_HOST: &str = "postman-echo.com";

/// Validate that the requested model is one this provider serves. The host
/// calls this before [`run`]; an unknown model is a hard error rather than a
/// silent default.
pub fn validate_model(model: &str) -> Result<String, String> {
    if ACCEPTED_MODELS.contains(&model) {
        Ok(model.to_string())
    } else {
        Err(format!(
            "anseo-example-provider does not accept model `{model}` \
             (accepted: {ACCEPTED_MODELS:?})"
        ))
    }
}

/// Build the echo endpoint URL for a prompt. A real provider would build its
/// upstream request here; the echo API simply reflects the input so the example
/// is deterministic and needs no credentials.
pub fn echo_url(prompt: &str) -> String {
    // The host enforces that the resolved host stays within the declared
    // `network` allowlist (= ECHO_HOST).
    let encoded = prompt.replace(' ', "+");
    format!("https://{ECHO_HOST}/get?prompt={encoded}")
}

/// The plugin entry point. In the deployed WASM build the host invokes this
/// across the SDK ABI boundary; here it is plain Rust so the template is
/// readable and unit-testable.
pub fn run(request: &ProviderRequest) -> Result<ProviderResponse, String> {
    let model = validate_model(&request.model)?;
    // A real provider performs `host:http/fetch(echo_url(&request.prompt))`
    // here. The echo API reflects the prompt, so the response is deterministic.
    //
    // Build the raw payload with serde_json so the prompt is JSON-escaped: a
    // prompt containing `"`, `\`, or control characters would otherwise produce
    // invalid JSON or inject structure into `raw_response`.
    let raw = serde_json::json!({
        "args": { "prompt": request.prompt },
        "model": model,
    })
    .to_string();
    Ok(ProviderResponse {
        message_text: request.prompt.clone(),
        raw_response: raw,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_known_model_rejects_unknown() {
        assert!(validate_model("echo-1").is_ok());
        assert!(validate_model("gpt-nope").is_err());
    }

    #[test]
    fn echo_url_stays_within_allowlisted_host() {
        let url = echo_url("hello world");
        assert!(url.starts_with(&format!("https://{ECHO_HOST}/")));
        assert!(url.contains("prompt=hello+world"));
    }

    #[test]
    fn run_echoes_prompt() {
        let req = ProviderRequest {
            prompt: "ping".to_string(),
            model: "echo-1".to_string(),
        };
        let resp = run(&req).expect("run succeeds");
        assert_eq!(resp.message_text, "ping");
        assert!(resp.raw_response.contains("ping"));
    }

    #[test]
    fn run_escapes_prompt_into_valid_json() {
        // A prompt with a quote and a backslash must not break out of the JSON
        // string or produce malformed JSON in `raw_response`.
        let prompt = r#"say "hi" \ now"#;
        let req = ProviderRequest {
            prompt: prompt.to_string(),
            model: "echo-1".to_string(),
        };
        let resp = run(&req).expect("run succeeds");

        // raw_response parses as JSON and round-trips the exact prompt verbatim.
        let parsed: serde_json::Value =
            serde_json::from_str(&resp.raw_response).expect("raw_response is valid JSON");
        assert_eq!(parsed["args"]["prompt"], serde_json::json!(prompt));
        assert_eq!(parsed["model"], serde_json::json!("echo-1"));
    }
}
