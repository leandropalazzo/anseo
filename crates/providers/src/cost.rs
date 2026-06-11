//! Provider model cost metadata used by Phase 2 schedule projections.

use anseo_core::ProviderName;

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

    #[test]
    fn plugin_providers_have_zero_first_party_cost() {
        // Plugin providers are excluded from first-party cost projection: their
        // blended rate is exactly 0, so per-run estimate is 0 too.
        let plugin = ProviderName::Plugin("custom-llm".to_string());
        let cost = default_model_cost(&plugin);
        assert_eq!(cost.usd_per_million_tokens, 0.0);
        assert_eq!(estimate_run_cost_usd(&plugin, 5_000), 0.0);
        // The model string is the provider's declared default.
        assert_eq!(cost.model, plugin.default_model());
    }

    #[test]
    fn estimate_run_cost_is_linear_in_tokens() {
        // Cost is (tokens / 1M) * rate. OpenAI is $7.50 / 1M tokens.
        let rate = default_model_cost(&ProviderName::Openai).usd_per_million_tokens;
        // 1M tokens → exactly the per-million rate.
        assert!((estimate_run_cost_usd(&ProviderName::Openai, 1_000_000) - rate).abs() < 1e-9);
        // Half a million → half the rate.
        assert!((estimate_run_cost_usd(&ProviderName::Openai, 500_000) - rate / 2.0).abs() < 1e-9);
        // Zero tokens → zero cost.
        assert_eq!(estimate_run_cost_usd(&ProviderName::Openai, 0), 0.0);
    }

    #[test]
    fn projection_with_no_providers_is_zero_cost_and_zero_runs() {
        // No enabled providers ⇒ nothing runs and nothing costs, even with
        // prompts and ticks configured. Guards against a divide/empty-sum bug.
        let projection = project_monthly_cost(&[], 10, 4.0);
        assert_eq!(projection.runs_per_month, 0.0);
        assert_eq!(projection.usd_per_run, 0.0);
        assert_eq!(projection.projected_monthly_usd, 0.0);
        assert_eq!(
            projection.estimated_tokens_per_run,
            DEFAULT_ESTIMATED_TOKENS_PER_RUN
        );
    }

    #[test]
    fn projection_with_no_prompts_is_zero() {
        // Zero prompts ⇒ zero runs and zero projected spend regardless of the
        // provider cohort or tick rate.
        let projection = project_monthly_cost(&[ProviderName::Openai], 0, 24.0);
        assert_eq!(projection.runs_per_month, 0.0);
        assert_eq!(projection.projected_monthly_usd, 0.0);
        // usd_per_run is per-provider-per-run, independent of prompt count.
        assert!(projection.usd_per_run > 0.0);
    }

    #[test]
    fn runs_per_month_uses_average_days_and_counts_every_provider() {
        // runs_per_month = ticks/day * avg_days * prompts * providers.
        let providers = [
            ProviderName::Openai,
            ProviderName::Gemini,
            ProviderName::Grok,
        ];
        let projection = project_monthly_cost(&providers, 5, 2.0);
        let expected = 2.0 * AVERAGE_DAYS_PER_MONTH * 5.0 * providers.len() as f64;
        assert!((projection.runs_per_month - expected).abs() < 1e-6);
    }

    #[test]
    fn usd_per_run_is_sum_of_per_provider_costs() {
        // usd_per_run sums each provider's single-run estimate at the default
        // token count — independent of prompt count and tick rate.
        let providers = [ProviderName::Openai, ProviderName::Mistral];
        let projection = project_monthly_cost(&providers, 7, 3.0);
        let expected: f64 = providers
            .iter()
            .map(|p| estimate_run_cost_usd(p, DEFAULT_ESTIMATED_TOKENS_PER_RUN))
            .sum();
        assert!((projection.usd_per_run - expected).abs() < 1e-9);
    }

    #[test]
    fn every_first_party_provider_has_a_distinct_positive_rate() {
        // Sanity that the rate table is populated and the headline providers are
        // not accidentally all the same value.
        let oa = default_model_cost(&ProviderName::Openai).usd_per_million_tokens;
        let an = default_model_cost(&ProviderName::Anthropic).usd_per_million_tokens;
        let mi = default_model_cost(&ProviderName::Mistral).usd_per_million_tokens;
        assert!(oa > 0.0 && an > 0.0 && mi > 0.0);
        // Anthropic is the priciest, Mistral the cheapest in the default table.
        assert!(an > oa);
        assert!(mi < oa);
    }
}
