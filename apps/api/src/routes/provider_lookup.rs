use std::sync::Arc;

use anseo_core::ProviderName;
use anseo_providers::{registry::build_real_registry, Provider};
use axum::http::StatusCode;
use axum::Json;

use crate::AppState;

type ApiError = (StatusCode, Json<serde_json::Value>);

fn err(code: StatusCode, error: &str, message: String) -> ApiError {
    (
        code,
        Json(serde_json::json!({ "error": error, "message": message })),
    )
}

fn boot_provider(state: &AppState, provider_name: &ProviderName) -> Option<Arc<dyn Provider>> {
    state
        .provider_registry
        .as_ref()
        .and_then(|registry| registry.get(provider_name))
        .cloned()
}

/// Resolve a provider from the current secret store when possible.
///
/// The boot registry can be stale after an operator saves a key in Settings.
/// Rebuilding from the boot config lets one-off AI suggestion routes use newly
/// stored credentials immediately while keeping the boot registry as a fallback.
pub async fn provider_for_request(
    state: &AppState,
    provider_name: &ProviderName,
    requested_provider: &str,
) -> Result<Arc<dyn Provider>, ApiError> {
    if let Some(config) = state.config.clone() {
        let config = (*config).clone();
        let join = tokio::task::spawn_blocking(move || build_real_registry(&config)).await;
        let registry = match join {
            Ok(Ok(registry)) => registry,
            Ok(Err(e)) => {
                if let Some(provider) = boot_provider(state, provider_name) {
                    return Ok(provider);
                }
                return Err(err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "secret_store_error",
                    e.to_string(),
                ));
            }
            Err(e) => {
                if let Some(provider) = boot_provider(state, provider_name) {
                    return Ok(provider);
                }
                return Err(err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "provider_registry_refresh_failed",
                    e.to_string(),
                ));
            }
        };
        if let Some(provider) = registry.get(provider_name) {
            return Ok(provider.clone());
        }
    }

    if let Some(provider) = boot_provider(state, provider_name) {
        return Ok(provider);
    } else if state.config.is_none() {
        return Err(err(
            StatusCode::SERVICE_UNAVAILABLE,
            "no_registry",
            "API booted without a provider registry; configure a provider key first".to_string(),
        ));
    }

    Err(err(
        StatusCode::BAD_REQUEST,
        "provider_not_configured",
        format!("provider `{requested_provider}` has no configured API key"),
    ))
}
