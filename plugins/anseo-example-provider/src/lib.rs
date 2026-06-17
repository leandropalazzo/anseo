//! `anseo-example-provider` — the canonical first-party Provider plugin and the
//! SDK author template (Story 41.5).
//!
//! This is the minimal shape a Provider plugin takes: it receives a prompt
//! request, performs its work, and returns a response. To stay deterministic —
//! and because the plugin sandbox forbids sockets (see
//! `crates/plugin-host/src/subprocess.rs`) — this template's work is a pure,
//! OFFLINE echo: it returns the prompt verbatim plus a JSON-escaped raw payload.
//! It makes NO network calls and declares NO `network` capability. A real
//! provider that needed an upstream API would add a `network` capability to
//! `manifest.yaml` and perform a host-mediated fetch; the [`upstream_url`] helper
//! shows where that request would be built.
//!
//! Build target: `wasm32-wasi` → `entrypoint.wasm` (the manifest `entry_point`).
//! The 41.4 CI pipeline compiles this, computes `SHA-256(manifest.yaml ||
//! entrypoint.wasm)`, signs it with the namespace author key, and publishes the
//! bundle + `signature.bin` + `claim.toml` to `github.com/leandropalazzo/plugin-registry`.
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

/// Illustrative upstream host a *networked* provider might call. This template
/// is OFFLINE and never reaches it; the constant exists only so [`upstream_url`]
/// can demonstrate where a real provider would build its request. If you make a
/// real networked provider, declare a matching `network` capability in
/// `manifest.yaml`; the host mediates every fetch against that allowlist.
pub const EXAMPLE_UPSTREAM_HOST: &str = "postman-echo.com";

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

/// Illustrative-only: build the URL a *networked* provider would request. This
/// template does NOT call it (the [`run`] path is fully offline); the helper
/// exists so authors can see where an upstream request is constructed. A real
/// provider would pass this to a host-mediated `host:http/fetch` after declaring
/// a matching `network` capability.
pub fn upstream_url(prompt: &str) -> String {
    let encoded = prompt.replace(' ', "+");
    format!("https://{EXAMPLE_UPSTREAM_HOST}/get?prompt={encoded}")
}

/// The plugin entry point. In the deployed WASM build the host invokes this
/// across the SDK ABI boundary; here it is plain Rust so the template is
/// readable and unit-testable.
pub fn run(request: &ProviderRequest) -> Result<ProviderResponse, String> {
    let model = validate_model(&request.model)?;
    // This template is OFFLINE: it echoes the prompt rather than calling out. A
    // networked provider would here perform a host-mediated
    // `host:http/fetch(upstream_url(&request.prompt))` (after declaring a
    // `network` capability) and use the response instead.
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
    fn upstream_url_builds_illustrative_request() {
        let url = upstream_url("hello world");
        assert!(url.starts_with(&format!("https://{EXAMPLE_UPSTREAM_HOST}/")));
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
