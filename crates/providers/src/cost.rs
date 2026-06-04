//! Provider model cost metadata used by Phase 2 schedule projections.

use opengeo_core::ProviderName;

/// Conservative blended token estimate per prompt run used for pre-run schedule
/// projection. Real usage accounting still comes from persisted provider rows.
pub const DEFAULT_ESTIMATED_TOKENS_PER_RUN: u64 = 2_000;

/// Phase 2 default project cap for scheduled runs.
pub const DEFAULT_PROJECT_MONTHLY_CAP_USD: f64 = 50.0;

/// Approximate average days/month used for cron projection.
pub const AVERAGE_DAYS_PER_MONTH: f64 = 30.4375;

#[derive(Debug, Clone, PartialEq)]
pub struct ModelCost {
    pub provider: ProviderName,
    pub model: &'static str,
    /// Blended USD cost per 1M tokens for schedule planning.
    pub usd_per_million_tokens: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CostProjection {
    pub runs_per_month: f64,
    pub estimated_tokens_per_run: u64,
    pub usd_per_run: f64,
    pub projected_monthly_usd: f64,
}

pub fn default_model_cost(provider: &ProviderName) -> ModelCost {
    let usd_per_million_tokens = match provider {
        ProviderName::Openai => 7.50,
        ProviderName::Anthropic => 9.00,
        ProviderName::Gemini => 5.25,
        ProviderName::Perplexity => 5.00,
        ProviderName::Grok => 7.00,
        ProviderName::Mistral => 4.00,
        ProviderName::Openrouter => 7.50,
        // Plugin providers are not part of first-party cost projection.
        ProviderName::Plugin(_) => 0.0,
    };
    ModelCost {
        provider: provider.clone(),
        model: provider.default_model(),
        usd_per_million_tokens,
    }
}

pub fn estimate_run_cost_usd(provider: &ProviderName, estimated_tokens: u64) -> f64 {
    let cost = default_model_cost(provider);
    (estimated_tokens as f64 / 1_000_000.0) * cost.usd_per_million_tokens
}

pub fn project_monthly_cost(
    providers: &[ProviderName],
    prompt_count: usize,
    ticks_per_day: f64,
) -> CostProjection {
    let runs_per_month = ticks_per_day * AVERAGE_DAYS_PER_MONTH * prompt_count as f64;
    let usd_per_tick: f64 = providers
        .iter()
        .map(|provider| estimate_run_cost_usd(provider, DEFAULT_ESTIMATED_TOKENS_PER_RUN))
        .sum::<f64>()
        * prompt_count as f64;
    CostProjection {
        runs_per_month: runs_per_month * providers.len() as f64,
        estimated_tokens_per_run: DEFAULT_ESTIMATED_TOKENS_PER_RUN,
        usd_per_run: providers
            .iter()
            .map(|provider| estimate_run_cost_usd(provider, DEFAULT_ESTIMATED_TOKENS_PER_RUN))
            .sum(),
        projected_monthly_usd: usd_per_tick * AVERAGE_DAYS_PER_MONTH * ticks_per_day,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_costs_cover_every_phase_2_provider() {
        for name in ProviderName::all_wire_names() {
            let provider = ProviderName::parse(name).unwrap();
            let cost = default_model_cost(&provider);
            assert_eq!(cost.provider, provider);
            assert_eq!(cost.model, provider.default_model());
            assert!(cost.usd_per_million_tokens > 0.0);
        }
    }

    #[test]
    fn monthly_projection_scales_by_prompt_and_provider_count() {
        let projection =
            project_monthly_cost(&[ProviderName::Openai, ProviderName::Anthropic], 2, 1.0);
        assert!((projection.runs_per_month - 121.75).abs() < 0.01);
        assert!(projection.projected_monthly_usd > 0.0);
    }
}
